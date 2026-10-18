//! Remote review orchestration. All network calls happen outside database transactions.
use super::ReviewService;
use crate::{
    error::{Error, Result, invalid},
    forge::{Github, Repository},
    models::{Actor, Anchor, Comment, Completion, Context, Session, Thread},
    utils::{PostReceipt, PullRequest, WorkspaceFile},
};
use serde_json::{Value, json};
use uuid::Uuid;

/// Reuses session completion keys and per-comment posting receipts for remote recovery.
#[derive(Clone)]
pub struct ForgeService {
    reviews: ReviewService,
    provider: Option<Github>,
}

struct Submission<'a> {
    repo: &'a Repository,
    pr: i64,
    session: &'a Session,
    comments: &'a [Comment],
    threads: &'a [Thread],
    account: &'a str,
    event: &'a str,
    body: &'a str,
}

impl ForgeService {
    async fn get_provider(&self) -> Result<Github> {
        match &self.provider {
            Some(p) => Ok(p.clone()),
            None => Github::from_environment().await,
        }
    }

    pub async fn get_pull_requests(
        &self,
        repository_url: &str,
        query: &str,
        page: u32,
    ) -> Result<(Vec<PullRequest>, u32)> {
        let repo = Repository::parse(repository_url)?;
        self.get_provider()
            .await?
            .get_pull_requests(&repo, query, page)
            .await
    }

    pub async fn refresh(&self, context_id: &Uuid) -> Result<Context> {
        let context = self.reviews.get_context_by_id(context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let result = async {
            let snapshot = self.get_provider().await?.get_snapshot(&repo, pr).await?;
            self.reviews.apply_remote(&context, snapshot, false).await
        }
        .await;
        if let Err(e) = &result {
            self.reviews
                .record_refresh_failure(&context, &e.to_string())
                .await?;
        }
        result
    }

    /// Read both revision sides from GitHub without checking out or trusting a local worktree.
    pub async fn get_workspace(
        &self,
        context_id: &Uuid,
    ) -> Result<(String, Vec<(WorkspaceFile, String)>)> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let context = self.reviews.get_context_by_id(context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        let pull = github.get_pull_request(&repo, pr).await?;
        if context.refreshed_head.as_deref() != Some(&pull.head) {
            return Err(Error::Conflict(
                "PR head changed; refresh before reading content".into(),
            ));
        }
        let files = github
            .get_all(&format!("{}/pulls/{pr}/files", repo.route()))
            .await?;
        if files.len() >= 3000 {
            return Err(invalid(
                "GitHub file listing may be truncated; narrow the review before proceeding",
            ));
        }
        let mut output = vec![];
        for file in files {
            let path = file["filename"]
                .as_str()
                .ok_or_else(|| invalid("GitHub file missing path"))?;
            Anchor::File { path: path.into() }.validate()?;
            let status = file["status"]
                .as_str()
                .ok_or_else(|| invalid("GitHub file missing status"))?;
            let previous = file["previous_filename"].as_str().unwrap_or(path);
            Anchor::File {
                path: previous.into(),
            }
            .validate()?;
            let mut sides = vec![];
            for (name, revision, absent) in [
                (previous, &pull.base, status == "added"),
                (path, &pull.head, status == "removed"),
            ] {
                if absent {
                    sides.push(String::new());
                    continue;
                }
                let encoded = name
                    .split('/')
                    .map(|part| {
                        url::form_urlencoded::byte_serialize(part.as_bytes())
                            .collect::<String>()
                            .replace('+', "%20")
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let result = github
                    .get(
                        &format!("{}/contents/{encoded}", repo.route()),
                        Some(&json!({"ref":revision})),
                    )
                    .await?;
                if result["size"].as_u64().is_none_or(|n| n > 1024 * 1024) {
                    return Err(invalid("Remote file exceeds supported 1 MiB text limit"));
                }
                if result["encoding"] != "base64" {
                    return Err(invalid("Remote content encoding unsupported"));
                }
                let encoded = result["content"]
                    .as_str()
                    .ok_or_else(|| invalid("GitHub content missing"))?
                    .replace(['\n', '\r'], "");
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| invalid("Malformed GitHub content encoding"))?;
                if bytes.contains(&0) {
                    return Err(invalid("Binary remote file is unsupported"));
                }
                sides.push(
                    String::from_utf8(bytes)
                        .map_err(|_| invalid("Remote file is not UTF-8 text"))?,
                );
            }
            let content = sides.pop().unwrap();
            let base = sides.pop().unwrap();
            output.push((
                WorkspaceFile {
                    path: path.into(),
                    content_hash: format!("sha256:{:x}", Sha256::digest(content.as_bytes())),
                    content,
                    status: status.into(),
                },
                base,
            ));
        }
        if github.get_pull_request(&repo, pr).await?.head != pull.head {
            return Err(Error::Conflict(
                "PR head changed during content fetch".into(),
            ));
        }
        Ok((repo.repository_url, output))
    }

    /// Freeze local payload before the first remote write; replay never starts a second review.
    pub async fn submit(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        outcome: &str,
        body: &str,
        request_key: &str,
        expected_head: &str,
    ) -> Result<Session> {
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let event = event(outcome)?;
        if request_key.trim().is_empty() {
            return Err(invalid("Submission requires a retry key"));
        }
        let session = self.reviews.get_session(session_id).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        if session.creator.id != actor.id {
            return Err(invalid("Only the session creator can submit this review"));
        }
        let input = Completion {
            outcome: Some(outcome.into()),
            body: Some(body.into()),
            submit: true,
        };
        if session.completion().is_some() {
            // Existing completion performs exact key/payload/actor validation.
            self.reviews
                .complete(&context.id, &actor, input, request_key)
                .await?;
            return self.reconcile(session_id, &actor).await;
        }
        if expected_head.is_empty() || session.commits.head.as_deref() != Some(expected_head) {
            return Err(Error::Conflict(
                "Expected PR head must match the reviewed session head".into(),
            ));
        }
        let github = self.get_provider().await?;
        if github.get_pull_request(&repo, pr).await?.head != expected_head {
            return Err(Error::Conflict(
                "PR head changed; refresh and review before submitting".into(),
            ));
        }
        let account = github.get_account_id().await?;
        let conversation = self.reviews.get_conversation(&context.id, true).await?;
        let mut comments: Vec<_> = conversation
            .comments
            .into_iter()
            .filter(|c| c.origin_session_id == Some(*session_id))
            .collect();
        comments.sort_by_key(|c| (c.position, c.created_at.clone(), c.id));
        // Validate all mappings before changing any posting state.
        for c in &comments {
            let thread = conversation
                .threads
                .iter()
                .find(|t| t.id == c.thread_id)
                .ok_or_else(|| invalid("Comment thread missing"))?;
            if let Anchor::Line { commit, side, .. } = &thread.anchor {
                let reviewed = if side == "old" {
                    session.commits.base.as_deref()
                } else {
                    session.commits.head.as_deref()
                };
                if reviewed != Some(commit.as_str()) {
                    return Err(Error::Conflict(
                        "Comment references an outdated revision".into(),
                    ));
                }
            }
        }
        let prepared: Vec<_> = comments
            .iter()
            .map(|c| {
                (
                    c.clone(),
                    format!("{}\n\n{}", render(c), comment_marker(*session_id, c.id)),
                    comment_key(session_id, &c.id),
                )
            })
            .collect();
        let frozen = self
            .reviews
            .prepare_submission(session_id, &actor, input, request_key, &prepared)
            .await?;
        let result = self
            .send(
                &github,
                Submission {
                    repo: &repo,
                    pr,
                    session: &frozen,
                    comments: &comments,
                    threads: &conversation.threads,
                    account: &account,
                    event,
                    body,
                },
            )
            .await;
        match result {
            Ok(review_id) => {
                self.reviews
                    .database()
                    .sessions
                    .update_submission(session_id, "sending", "confirmed", None, Some(&review_id))
                    .await?;
                self.reviews.get_session(session_id).await
            }
            Err(error) => {
                let current = self.reviews.get_session(session_id).await?;
                self.reviews
                    .database()
                    .sessions
                    .update_submission(
                        session_id,
                        "sending",
                        if definite_rejection(&error) {
                            "failed"
                        } else {
                            "uncertain"
                        },
                        Some(&error.to_string()),
                        current.forge_review_id.as_deref(),
                    )
                    .await?;
                for c in &comments {
                    if self
                        .reviews
                        .get_comment(&context.id, &c.id)
                        .await?
                        .forge_comment_id
                        .is_none()
                    {
                        self.reviews
                            .posting_uncertain(&context.id, &c.id, &error.to_string())
                            .await?;
                    }
                }
                // Return durable uncertain state, not an invented failed/confirmed result.
                self.reviews.get_session(session_id).await
            }
        }
    }

    async fn send(&self, github: &Github, submission: Submission<'_>) -> Result<String> {
        let Submission {
            repo,
            pr,
            session,
            comments,
            threads,
            account,
            event,
            body,
        } = submission;
        let marker = review_marker(session.id);
        let review_body = format!("{body}\n\n{marker}");
        let route = format!("{}/pulls/{pr}/reviews", repo.route());
        let review = github
            .post(
                &route,
                &json!({"commit_id":session.commits.head,"body":review_body}),
            )
            .await?;
        let review_id = crate::forge::github::opaque(&review, "id")?;
        let node_id = review["node_id"]
            .as_str()
            .ok_or_else(|| invalid("Pending review receipt lacks node ID"))?;
        if !self
            .reviews
            .database()
            .sessions
            .update_submission(&session.id, "sending", "sending", None, Some(&review_id))
            .await?
        {
            return Err(Error::Conflict(
                "Submission state changed after pending review creation".into(),
            ));
        }
        for c in comments {
            let thread = threads
                .iter()
                .find(|t| t.id == c.thread_id)
                .ok_or_else(|| invalid("Comment thread missing"))?;
            let body = format!("{}\n\n{}", render(c), comment_marker(session.id, c.id));
            let receipt = if c.position > 0 {
                let root = threads
                    .iter()
                    .find(|t| t.id == c.thread_id)
                    .ok_or_else(|| invalid("Reply thread missing"))?;
                let conversation = self
                    .reviews
                    .get_conversation(&session.context_id, true)
                    .await?;
                let root_comment = conversation
                    .comments
                    .iter()
                    .find(|v| v.thread_id == root.id && v.position == 0)
                    .ok_or_else(|| invalid("Reply root missing"))?;
                if root_comment.forge_comment_id.is_none()
                    || root_comment.forge_object_kind.as_deref() != Some("review_comment")
                {
                    return Err(invalid(
                        "GitHub replies require an existing review-comment thread",
                    ));
                }
                let thread_id = conversation
                    .threads
                    .iter()
                    .find(|t| t.id == root.id)
                    .and_then(|t| t.forge_thread_id.as_deref())
                    .ok_or_else(|| invalid("Reply thread has no verified GitHub node ID"))?;
                let response = github.graphql(json!({"query":"mutation($input:AddPullRequestReviewThreadReplyInput!){addPullRequestReviewThreadReply(input:$input){comment{databaseId}}}","variables":{"input":{"pullRequestReviewId":node_id,"pullRequestReviewThreadId":thread_id,"body":body}}})).await?;
                PostReceipt {
                    comment_id: crate::forge::github::opaque(
                        &response["addPullRequestReviewThreadReply"]["comment"],
                        "databaseId",
                    )?,
                    kind: "review_comment".into(),
                    thread_id: Some(thread_id.into()),
                    review_id: Some(review_id.clone()),
                    posting_account_id: account.into(),
                }
            } else {
                match &thread.anchor {
                    Anchor::General => {
                        let response = github
                            .post(
                                &format!("{}/issues/{pr}/comments", repo.route()),
                                &json!({"body":body}),
                            )
                            .await?;
                        PostReceipt {
                            comment_id: crate::forge::github::opaque(&response, "id")?,
                            kind: "conversation_comment".into(),
                            thread_id: None,
                            review_id: None,
                            posting_account_id: account.into(),
                        }
                    }
                    anchor => {
                        let mut input = json!({"pullRequestReviewId":node_id,"body":body});
                        match anchor {
                            Anchor::File { path } => {
                                input["path"] = json!(path);
                                input["subjectType"] = json!("FILE");
                            }
                            Anchor::Line {
                                path,
                                start,
                                end,
                                side,
                                ..
                            } => {
                                input["path"] = json!(path);
                                input["subjectType"] = json!("LINE");
                                input["line"] = json!(end);
                                input["side"] = json!(if side == "old" { "LEFT" } else { "RIGHT" });
                                if start != end {
                                    input["startLine"] = json!(start);
                                    input["startSide"] = input["side"].clone();
                                }
                            }
                            Anchor::General => unreachable!(),
                        }
                        let response=github.graphql(json!({"query":"mutation($input:AddPullRequestReviewThreadInput!){addPullRequestReviewThread(input:$input){thread{id comments(first:1){nodes{databaseId}}}}}","variables":{"input":input}})).await?;
                        let t = &response["addPullRequestReviewThread"]["thread"];
                        PostReceipt {
                            comment_id: crate::forge::github::opaque(
                                &t["comments"]["nodes"][0],
                                "databaseId",
                            )?,
                            kind: "review_comment".into(),
                            thread_id: Some(
                                t["id"]
                                    .as_str()
                                    .ok_or_else(|| invalid("Thread receipt missing ID"))?
                                    .into(),
                            ),
                            review_id: Some(review_id.clone()),
                            posting_account_id: account.into(),
                        }
                    }
                }
            };
            self.reviews
                .confirm_post(
                    &session.context_id,
                    &c.id,
                    &comment_key(&session.id, &c.id),
                    receipt,
                )
                .await?;
        }
        if github.get_pull_request(repo, pr).await?.head
            != session.commits.head.as_deref().unwrap_or("")
        {
            return Err(Error::Conflict(
                "PR head changed before final review submission".into(),
            ));
        }
        let response = github
            .post(
                &format!("{route}/{review_id}/events"),
                &json!({"event":event,"body":review_body}),
            )
            .await?;
        if !state_matches(&response, event) {
            return Err(invalid(
                "Review submission response did not confirm requested outcome",
            ));
        }
        Ok(review_id)
    }

    /// Verify receipts; explicitly finalize a known pending review only when every payload is accounted for.
    /// Missing/ambiguous comments are never replayed.
    pub async fn reconcile(&self, session_id: &Uuid, actor: &Actor) -> Result<Session> {
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let session = self.reviews.get_session(session_id).await?;
        if actor.id != session.creator.id {
            return Err(invalid("Only the session creator may reconcile submission"));
        }
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        if session.submission_status == "confirmed" {
            return Ok(session);
        }
        let github = self.get_provider().await?;
        let account = github.get_account_id().await?;
        let reviews = github
            .get_all(&format!("{}/pulls/{pr}/reviews", repo.route()))
            .await?;
        let marker = review_marker(*session_id);
        let completion = session
            .completion()
            .ok_or_else(|| invalid("Session has no frozen submission"))?;
        let expected_body = format!("{}\n\n{marker}", completion.body.as_deref().unwrap_or(""));
        let matches: Vec<_> = reviews
            .iter()
            .filter(|v| {
                v["body"].as_str() == Some(expected_body.as_str())
                    && crate::forge::github::opaque(&v["user"], "id")
                        .ok()
                        .as_deref()
                        == Some(&account)
            })
            .collect();
        if matches.len() != 1 {
            return Err(Error::Conflict(
                "Submission cannot be reconciled uniquely; no write retried".into(),
            ));
        }
        let remote = matches[0];
        let id = crate::forge::github::opaque(remote, "id")?;
        if session
            .forge_review_id
            .as_ref()
            .is_some_and(|stored| stored != &id)
        {
            return Err(Error::Conflict(
                "Remote receipt conflicts with stored review ID".into(),
            ));
        }
        let final_event = event(&completion.outcome)?;
        let pending = remote["state"] == "PENDING";
        if remote["commit_id"].as_str() != session.commits.head.as_deref()
            || (!pending && !state_matches(remote, final_event))
        {
            return Err(Error::Conflict("Review commit/outcome differs from the frozen submission; inspect it in GitHub before starting a new session".into()));
        }
        if pending && !["uncertain", "failed"].contains(&session.submission_status.as_str()) {
            return Err(Error::Conflict("Submission may still be running; wait and reconcile again. After a backend crash, inspect the pending review in GitHub before manual recovery".into()));
        }
        // Confirm exact staged payload plus account and a unique provider receipt.
        let conversation = self.reviews.get_conversation(&context.id, true).await?;
        let line = github
            .get_all(&format!(
                "{}/pulls/{pr}/reviews/{id}/comments",
                repo.route()
            ))
            .await?;
        let general = github
            .get_all(&format!("{}/issues/{pr}/comments", repo.route()))
            .await?;
        for c in conversation
            .comments
            .iter()
            .filter(|c| c.origin_session_id == Some(*session_id) && c.forge_comment_id.is_none())
        {
            let payload = c
                .forge_body
                .as_deref()
                .ok_or_else(|| invalid("Missing staged comment payload"))?;
            let found: Vec<_> = line
                .iter()
                .map(|v| (v, "review_comment"))
                .chain(general.iter().map(|v| (v, "conversation_comment")))
                .filter(|(v, _)| {
                    v["body"].as_str() == Some(payload)
                        && crate::forge::github::opaque(&v["user"], "id")
                            .ok()
                            .as_deref()
                            == Some(&account)
                })
                .collect();
            if found.len() != 1 {
                return Err(Error::Conflict(
                    "Comment receipt is missing or ambiguous; no writes retried. Inspect the pending review in GitHub, reconcile accepted comments, or discard that pending review manually before starting a new session".into(),
                ));
            }
            let (v, kind) = found[0];
            let remote_id = crate::forge::github::opaque(v, "id")?;
            if c.forge_comment_id
                .as_ref()
                .is_some_and(|stored| stored != &remote_id)
            {
                return Err(Error::Conflict(
                    "Stored comment receipt conflicts with provider ID; no write retried".into(),
                ));
            }
            self.reviews
                .confirm_post(
                    &context.id,
                    &c.id,
                    &comment_key(session_id, &c.id),
                    PostReceipt {
                        comment_id: crate::forge::github::opaque(v, "id")?,
                        kind: kind.into(),
                        thread_id: None,
                        review_id: (kind == "review_comment").then(|| id.clone()),
                        posting_account_id: account.clone(),
                    },
                )
                .await?;
        }
        let expected_status = if pending {
            if github.get_pull_request(&repo, pr).await?.head
                != session.commits.head.as_deref().unwrap_or("")
            {
                return Err(Error::Conflict("PR head changed; pending review was not submitted. Inspect it in GitHub before reworking the review".into()));
            }
            if !self
                .reviews
                .database()
                .sessions
                .update_submission(
                    session_id,
                    &session.submission_status,
                    "reconciling",
                    None,
                    Some(&id),
                )
                .await?
            {
                return Err(Error::Conflict(
                    "Another recovery owns this review; no write retried".into(),
                ));
            }
            let result = github
                .post(
                    &format!("{}/pulls/{pr}/reviews/{id}/events", repo.route()),
                    &json!({"event":final_event,"body":expected_body}),
                )
                .await;
            match result {
                Ok(response) if state_matches(&response, final_event) => {}
                result => {
                    let error = match result {
                        Err(error) => error,
                        Ok(_) => invalid("Recovery response did not confirm review outcome"),
                    };
                    self.reviews
                        .database()
                        .sessions
                        .update_submission(
                            session_id,
                            "reconciling",
                            if definite_rejection(&error) {
                                "failed"
                            } else {
                                "uncertain"
                            },
                            Some(&error.to_string()),
                            Some(&id),
                        )
                        .await?;
                    return self.reviews.get_session(session_id).await;
                }
            }
            "reconciling"
        } else {
            &session.submission_status
        };
        self.reviews
            .database()
            .sessions
            .update_submission(session_id, expected_status, "confirmed", None, Some(&id))
            .await?;
        self.reviews.get_session(session_id).await
    }

    pub async fn resolve(
        &self,
        session_id: &Uuid,
        thread_id: &Uuid,
        actor: &Actor,
        resolved: bool,
    ) -> Result<Thread> {
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let session = self.reviews.get_session(session_id).await?;
        let conversation = self
            .reviews
            .get_conversation(&session.context_id, true)
            .await?;
        let thread = conversation
            .threads
            .iter()
            .find(|t| t.id == *thread_id)
            .ok_or_else(|| Error::NotFound("Thread".into()))?;
        let Some(id) = &thread.forge_thread_id else {
            return self
                .reviews
                .resolve(session_id, thread_id, &actor, resolved)
                .await;
        };
        if matches!(thread.anchor, Anchor::General) {
            return Err(invalid("GitHub only supports resolving review threads"));
        }
        let mutation = if resolved {
            "resolveReviewThread"
        } else {
            "unresolveReviewThread"
        };
        self.get_provider().await?.graphql(json!({"query":format!("mutation($id:ID!){{{mutation}(input:{{threadId:$id}}){{thread{{id isResolved}}}}}}"),"variables":{"id":id}})).await?;
        self.refresh(&session.context_id).await?;
        self.reviews
            .get_conversation(&session.context_id, true)
            .await?
            .threads
            .into_iter()
            .find(|t| t.id == *thread_id)
            .ok_or_else(|| Error::NotFound("Thread after refresh".into()))
    }

    pub async fn edit_comment(
        &self,
        session_id: &Uuid,
        comment_id: &Uuid,
        actor: &Actor,
        body: &str,
    ) -> Result<Comment> {
        Comment::validate_body(body)?;
        let session = self.reviews.get_session(session_id).await?;
        let comment = self
            .reviews
            .get_comment(&session.context_id, comment_id)
            .await?;
        if comment.forge_comment_id.is_none() {
            return self
                .reviews
                .edit_comment(session_id, comment_id, actor, body)
                .await;
        }
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        check_owner(&comment, &actor, &github.get_account_id().await?)?;
        let route = comment_route(&repo, pr, &comment)?;
        let mut updated = comment.clone();
        updated.body = body.into();
        let rendered = if let Some(origin) = updated.origin_session_id {
            format!(
                "{}\n\n{}",
                render(&updated),
                comment_marker(origin, updated.id)
            )
        } else {
            body.to_owned()
        };
        github.patch(&route, &json!({"body":rendered})).await?;
        updated.forge_body = Some(rendered);
        updated.updated_at = crate::utils::now();
        self.reviews
            .database()
            .comments
            .update(&crate::database::models::comments::Model::from(&updated))
            .await?;
        self.refresh(&session.context_id).await?;
        self.reviews
            .get_comment(&session.context_id, comment_id)
            .await
    }

    pub async fn delete_comment(
        &self,
        session_id: &Uuid,
        comment_id: &Uuid,
        actor: &Actor,
    ) -> Result<()> {
        let session = self.reviews.get_session(session_id).await?;
        let comment = self
            .reviews
            .get_comment(&session.context_id, comment_id)
            .await?;
        if comment.forge_comment_id.is_none() {
            return self
                .reviews
                .delete_comment(session_id, comment_id, actor)
                .await;
        }
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        check_owner(&comment, &actor, &github.get_account_id().await?)?;
        if comment.forge_object_kind.as_deref() == Some("review") {
            return Err(invalid(
                "Submitted reviews cannot be deleted through comment deletion",
            ));
        }
        github.delete(&comment_route(&repo, pr, &comment)?).await?;
        self.refresh(&session.context_id).await?;
        Ok(())
    }

    pub fn new(reviews: ReviewService) -> Self {
        Self {
            reviews,
            provider: None,
        }
    }

    pub fn with_provider(reviews: ReviewService, provider: Github) -> Self {
        Self {
            reviews,
            provider: Some(provider),
        }
    }
}

fn event(outcome: &str) -> Result<&'static str> {
    match outcome {
        "approve" => Ok("APPROVE"),
        "reject" | "request_changes" => Ok("REQUEST_CHANGES"),
        "comment" => Ok("COMMENT"),
        _ => Err(invalid("Review outcome must be approve, reject or comment")),
    }
}
fn state_matches(v: &Value, event: &str) -> bool {
    v["state"].as_str()
        == Some(match event {
            "APPROVE" => "APPROVED",
            "REQUEST_CHANGES" => "CHANGES_REQUESTED",
            _ => "COMMENTED",
        })
}
fn review_marker(session: Uuid) -> String {
    format!("<!-- lgtm-review:{session} -->")
}
fn comment_marker(session: Uuid, comment: Uuid) -> String {
    format!("<!-- lgtm-comment:{session}:{comment} -->")
}
fn comment_key(session: &Uuid, comment: &Uuid) -> String {
    format!("{session}:{comment}")
}
fn render(c: &Comment) -> String {
    if c.author.model().is_some() {
        format!("{}\n\n> {}", c.body, c.author.label())
    } else {
        c.body.clone()
    }
}
fn check_owner(c: &Comment, actor: &Actor, account: &str) -> Result<()> {
    if (c.origin_session_id.is_some() && c.author.id != actor.id)
        || c.posting_account_id.as_deref() != Some(account)
    {
        Err(invalid(
            "Only the attributed author using its posting account may modify this comment",
        ))
    } else {
        Ok(())
    }
}
fn comment_route(repo: &Repository, pr: i64, c: &Comment) -> Result<String> {
    let id = c
        .forge_comment_id
        .as_deref()
        .ok_or_else(|| invalid("Remote comment ID missing"))?;
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Malformed remote comment ID"));
    }
    Ok(match c.forge_object_kind.as_deref() {
        Some("review_comment") => format!("{}/pulls/comments/{id}", repo.route()),
        Some("conversation_comment") => format!("{}/issues/comments/{id}", repo.route()),
        Some("review") => format!("{}/pulls/{pr}/reviews/{id}", repo.route()),
        _ => return Err(invalid("Unsupported remote comment kind")),
    })
}

/// Only an explicit rejection response proves the attempted write was rejected.
fn definite_rejection(error: &Error) -> bool {
    let message = error.to_string();
    message
        .strip_prefix("GitHub HTTP ")
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|status| (400..500).contains(&status) && status != 408)
}
