//! Remote review orchestration. All network calls happen outside database transactions.
use crate::extensions::gh;
mod changes;

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
    // The daemon owns one forge service; clones serialize draft and submit writes.
    writes: std::sync::Arc<tokio::sync::Mutex<()>>,
}

#[derive(Clone, Copy)]
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

    async fn validate_comparison(
        &self,
        github: &Github,
        repo: &Repository,
        pr: i64,
        session: &Session,
    ) -> Result<PullRequest> {
        let observed = github.get_pull_request(repo, pr).await?;
        if observed.state != crate::database::types::ForgeState::Open {
            return Err(Error::Conflict(
                "PR is closed or merged; saved feedback remains available".into(),
            ));
        }
        if session.commits.head.as_deref() != Some(observed.head.as_str())
            || session.commits.base.as_deref() != Some(observed.base.as_str())
        {
            return Err(Error::Conflict(
                "PR base or head changed; refresh and review before publishing".into(),
            ));
        }
        Ok(observed)
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
        let _write = self.writes.lock().await;
        self.refresh_context(context_id, None).await
    }

    pub async fn refresh_at_version(&self, context_id: &Uuid, version: i64) -> Result<Context> {
        let _write = self.writes.lock().await;
        self.refresh_context(context_id, Some(version)).await
    }

    /// Acceptance never deletes an existing provider review or repeats an uncertain intent.
    pub async fn refresh_for_acceptance(
        &self,
        context_id: &Uuid,
        review_id: &Uuid,
        actor: &Actor,
        expected_version: i64,
    ) -> Result<Context> {
        let _write = self.writes.lock().await;
        let session = self.reviews.get_session(review_id).await?;
        if session.context_id != *context_id
            || session.creator.id != actor.id
            || session.state() != "active"
            || session.submission_status != "unsubmitted"
        {
            return Err(Error::Conflict(
                "Recover the selected PR attempt before accepting another comparison".into(),
            ));
        }
        let context = self.reviews.get_context_by_id(context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        let account = github.get_account_id().await?;
        // The same owned pending review survives a push. An unrelated provider draft
        // must still be reconciled rather than adopted implicitly.
        self.get_pending(&github, &repo, pr, &session, &account)
            .await?;
        self.refresh_context(context_id, Some(expected_version))
            .await
    }

    async fn refresh_context(
        &self,
        context_id: &Uuid,
        expected_version: Option<i64>,
    ) -> Result<Context> {
        let mut context = self.reviews.get_context_by_id(context_id).await?;
        if expected_version.is_some_and(|version| version != context.revision) {
            return Err(Error::Conflict(
                "Review changed before the provider observation".into(),
            ));
        }
        let (repo, pr) = Repository::from_context(&context.key)?;
        let result = async {
            let github = self.get_provider().await?;
            let snapshot = github.get_snapshot(&repo, pr).await?;
            let conversation = self.reviews.get_conversation(context_id, true).await?;
            let missing_private: Vec<_> = conversation
                .comments
                .iter()
                .filter(|comment| {
                    comment.visibility == "draft"
                        && comment.forge_comment_id.is_some()
                        && !snapshot
                            .threads
                            .iter()
                            .flat_map(|thread| &thread.comments)
                            .any(|remote| {
                                Some(&remote.id) == comment.forge_comment_id.as_ref()
                                    && Some(&remote.kind) == comment.forge_object_kind.as_ref()
                            })
                })
                .collect();
            if !missing_private.is_empty() {
                let account = github.get_account_id().await?;
                if missing_private
                    .iter()
                    .any(|comment| comment.posting_account_id.as_deref() != Some(account.as_str()))
                {
                    return Err(Error::Conflict(
                        "GitHub account changed; private draft absence cannot confirm deletion"
                            .into(),
                    ));
                }
            }
            for c in conversation.comments.iter().filter(|c| {
                ["sending", "uncertain"].contains(&c.posting.state().as_str())
                    && c.visibility == "draft"
            }) {
                let account = github.get_account_id().await?;
                let session_id = c
                    .origin_session_id
                    .ok_or_else(|| invalid("Staged draft has no session"))?;
                self.confirm_draft_receipt(&snapshot, c, &account, &session_id)
                    .await?;
                // confirm_post advances this context once. Preserve its CAS boundary
                // while accounting for this operation's own receipt recovery.
                context.revision += 1;
            }
            // Recover an accepted pending-review creation before following a push.
            // The original marker/account/commit must prove ownership; never recreate it.
            if let Some(active) = self
                .reviews
                .database()
                .sessions
                .get_active(&context.id)
                .await?
                && ["draft_sending", "draft_uncertain"].contains(&active.submission_status.as_str())
            {
                let session = self.reviews.get_session(&active.id).await?;
                let account = github.get_account_id().await?;
                if let Some(pending) = self
                    .get_pending(&github, &repo, pr, &session, &account)
                    .await?
                {
                    let id = gh::opaque(&pending, "id")?;
                    if !self
                        .reviews
                        .database()
                        .sessions
                        .update_submission(
                            &session.id,
                            &session.submission_status,
                            "unsubmitted",
                            None,
                            Some(&id),
                        )
                        .await?
                    {
                        return Err(Error::Conflict(
                            "Pending review changed during recovery".into(),
                        ));
                    }
                }
            }
            let observed = self.reviews.apply_remote(&context, snapshot, false).await?;
            self.reviews.synchronize_pr_comparison(&observed).await
        }
        .await;
        if let Err(e) = &result {
            self.reviews
                .record_refresh_failure(&context, &e.to_string())
                .await?;
        }
        result
    }

    /// Compatibility adapter for the existing wire method during consumer migration.
    pub async fn get_workspace(
        &self,
        context_id: &Uuid,
        selected: &str,
        metadata_only: bool,
    ) -> Result<(String, Vec<(WorkspaceFile, String)>)> {
        let context = self.reviews.get_context_by_id(context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        super::diff::DiffService::with_provider(self.get_provider().await?)
            .get_remote_workspace(
                &repo,
                pr,
                context.refreshed_base.as_deref(),
                context.refreshed_head.as_deref().unwrap_or(""),
                selected,
                metadata_only,
            )
            .await
    }

    /// Persisted comments stay drafts until the explicit review submission.
    /// An uncertain write is recovered by exact receipt lookup, never replayed.
    pub async fn sync_draft(
        &self,
        session_id: &Uuid,
        comment_id: &Uuid,
        actor: &Actor,
    ) -> Result<Comment> {
        let _write = self.writes.lock().await;
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let mut session = self.reviews.get_session(session_id).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let comment = self.reviews.get_comment(&context.id, comment_id).await?;
        if session.state() != "active"
            || comment.owner_id != Some(actor.id)
            || comment.origin_session_id != Some(*session_id)
        {
            return Err(invalid(
                "Only your active review drafts can be synchronized",
            ));
        }
        if !context.key.is_forge_backed() {
            return Ok(comment);
        }
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        let observed = github.get_pull_request(&repo, pr).await?;
        if observed.state != crate::database::types::ForgeState::Open {
            return Err(Error::Conflict(
                "PR is closed or merged; saved feedback remains available".into(),
            ));
        }
        if session.commits.base.as_deref() != Some(observed.base.as_str())
            || session.commits.head.as_deref() != Some(observed.head.as_str())
        {
            self.refresh_context(&context.id, None).await?;
            session = self.reviews.get_session(session_id).await?;
        }
        let account = github.get_account_id().await?;
        // A push may become visible between the first metadata read and this check.
        // Refresh once more before any write; never retry an uncertain provider mutation.
        match self.validate_comparison(&github, &repo, pr, &session).await {
            Ok(_) => {}
            Err(Error::Conflict(_)) => {
                self.refresh_context(&context.id, None).await?;
                session = self.reviews.get_session(session_id).await?;
                self.validate_comparison(&github, &repo, pr, &session)
                    .await?;
            }
            Err(error) => return Err(error),
        }
        // Receipt recovery during refresh can have confirmed this draft already.
        let comment = self.reviews.get_comment(&context.id, comment_id).await?;
        let mut conversation = self.reviews.get_conversation(&context.id, true).await?;
        let thread = conversation
            .threads
            .iter()
            .find(|t| t.id == comment.thread_id)
            .ok_or_else(|| invalid("Comment thread missing"))?;
        if matches!(thread.anchor, Anchor::General) {
            return Ok(comment);
        }
        let payload = if comment.forge_comment_id.is_none()
            && !["sending", "uncertain"].contains(&comment.posting.state().as_str())
        {
            Some(
                self.get_posting_payload(&session, &comment, &mut conversation.threads)
                    .await?,
            )
        } else {
            None
        };
        let pending = if let Some(pending) = self
            .get_pending(&github, &repo, pr, &session, &account)
            .await?
        {
            pending
        } else {
            if session.forge_review_id.is_some()
                || !["unsubmitted", "draft_failed"].contains(&session.submission_status.as_str())
            {
                return Err(Error::Conflict("Pending review creation is uncertain; inspect GitHub before retrying. No write repeated".into()));
            }
            if !self
                .reviews
                .database()
                .sessions
                .update_submission(
                    session_id,
                    &session.submission_status,
                    "draft_sending",
                    None,
                    None,
                )
                .await?
            {
                return Err(Error::Conflict(
                    "Another request is preparing this pending review".into(),
                ));
            }
            let result = github
                .post(
                    &format!("{}/pulls/{pr}/reviews", repo.route()),
                    &json!({"commit_id":session.commits.head,"body":review_marker(session.id)}),
                )
                .await;
            match result {
                Ok(value) => value,
                Err(error) => {
                    self.reviews
                        .database()
                        .sessions
                        .update_submission(
                            session_id,
                            "draft_sending",
                            if definite_rejection(&error) {
                                "draft_failed"
                            } else {
                                "draft_uncertain"
                            },
                            Some(&error.to_string()),
                            None,
                        )
                        .await?;
                    return Err(error);
                }
            }
        };
        let review_id = gh::opaque(&pending, "id")?;
        let current = self.reviews.get_session(session_id).await?;
        if !self
            .reviews
            .database()
            .sessions
            .update_submission(
                session_id,
                &current.submission_status,
                "unsubmitted",
                None,
                Some(&review_id),
            )
            .await?
        {
            return Err(Error::Conflict("Pending review state changed".into()));
        }
        if let Some(id) = &comment.forge_comment_id {
            check_owner(&comment, &actor, &account)?;
            if comment.forge_review_id.as_deref() != Some(&review_id) {
                return Err(Error::Conflict(
                    "Comment belongs to another pending review".into(),
                ));
            }
            let remote = gh::get_all(
                &github.client,
                &format!("{}/pulls/{pr}/reviews/{review_id}/comments", repo.route()),
            )
            .await?;
            if !remote.iter().any(|v| {
                gh::opaque(v, "id").ok().as_ref() == Some(id)
                    && v["body"].as_str() == comment.forge_body.as_deref()
                    && gh::opaque(&v["user"], "id").ok().as_deref() == Some(account.as_str())
            }) {
                return Err(Error::Conflict(
                    "GitHub draft changed or disappeared; refresh before continuing".into(),
                ));
            }
            return Ok(comment);
        }
        if ["sending", "uncertain"].contains(&comment.posting.state().as_str()) {
            let snapshot = github.get_snapshot(&repo, pr).await?;
            self.confirm_draft_receipt(&snapshot, &comment, &account, session_id)
                .await?;
        } else {
            let payload = payload.ok_or_else(|| invalid("Draft payload unavailable"))?;
            self.reviews
                .stage_post(
                    session_id,
                    comment_id,
                    &comment_key(session_id, comment_id),
                    &comment,
                    &payload,
                )
                .await?;
            let submission = Submission {
                repo: &repo,
                pr,
                session: &session,
                comments: &[],
                threads: &conversation.threads,
                account: &account,
                event: "",
                body: "",
            };
            if let Err(error) = self
                .send_comment(&github, &submission, &comment, &pending)
                .await
            {
                if definite_rejection(&error) {
                    self.reviews.reject_post(&context.id, comment_id).await?;
                } else {
                    self.reviews
                        .posting_uncertain(&context.id, comment_id, &error.to_string())
                        .await?;
                }
                return Err(error);
            }
        }
        self.reviews.get_comment(&context.id, comment_id).await
    }

    async fn confirm_draft_receipt(
        &self,
        snapshot: &crate::utils::ForgeSnapshot,
        comment: &Comment,
        account: &str,
        session_id: &Uuid,
    ) -> Result<()> {
        let payload = comment
            .forge_body
            .as_deref()
            .ok_or_else(|| invalid("Missing staged draft payload"))?;
        let session = self.reviews.get_session(session_id).await?;
        let matches: Vec<_> = snapshot
            .threads
            .iter()
            .flat_map(|t| t.comments.iter().map(move |c| (t, c)))
            .filter(|(_, c)| {
                c.pending
                    && c.review_id == session.forge_review_id
                    && c.body == payload
                    && c.author.account_id == account
            })
            .collect();
        if matches.len() != 1 {
            return Err(Error::Conflict(
                "Draft receipt is missing or ambiguous; inspect GitHub. No comment was reposted"
                    .into(),
            ));
        }
        let (thread, remote) = matches[0];
        self.reviews
            .confirm_post(
                &comment.context_id,
                &comment.id,
                &comment_key(session_id, &comment.id),
                PostReceipt {
                    comment_id: remote.id.clone(),
                    kind: "review_comment".into(),
                    thread_id: Some(thread.id.clone()),
                    review_id: remote.review_id.clone(),
                    posting_account_id: account.into(),
                },
            )
            .await
    }

    async fn get_pending(
        &self,
        github: &Github,
        repo: &Repository,
        pr: i64,
        session: &Session,
        account: &str,
    ) -> Result<Option<Value>> {
        let reviews = gh::get_all(
            &github.client,
            &format!("{}/pulls/{pr}/reviews", repo.route()),
        )
        .await?;
        let owned: Vec<_> = reviews
            .into_iter()
            .filter(|v| {
                v["state"] == "PENDING"
                    && gh::opaque(&v["user"], "id").ok().as_deref() == Some(account)
            })
            .collect();
        if owned.len() > 1 {
            return Err(Error::Conflict(
                "Multiple pending reviews found; inspect GitHub".into(),
            ));
        }
        let Some(pending) = owned.into_iter().next() else {
            return Ok(None);
        };
        let id = gh::opaque(&pending, "id")?;
        if session.forge_review_id.as_ref() != Some(&id)
            && (pending["commit_id"].as_str() != session.commits.head.as_deref()
                || pending["body"].as_str() != Some(&review_marker(session.id)))
        {
            return Err(Error::Conflict("A different pending review already exists on GitHub; submit or discard it before starting this review".into()));
        }
        Ok(Some(pending))
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
        self.submit_review(
            session_id,
            actor,
            crate::models::ReviewSubmission {
                outcome: outcome.into(),
                body: body.into(),
                request_key: request_key.into(),
                expected_head: expected_head.into(),
                expected_base: None,
                expected_version: None,
            },
        )
        .await
    }

    pub async fn submit_review(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        intent: crate::models::ReviewSubmission,
    ) -> Result<Session> {
        let outcome = intent.outcome.as_str();
        let body = intent.body.as_str();
        let request_key = intent.request_key.as_str();
        let expected_head = intent.expected_head.as_str();
        let _write = self.writes.lock().await;
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let outcome = if outcome == "request_changes" {
            "reject"
        } else {
            outcome
        };
        let event = event(outcome)?;
        if request_key.trim().is_empty() {
            return Err(invalid("Submission requires a retry key"));
        }
        let session = self.reviews.get_session(session_id).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        if self
            .reviews
            .database()
            .mutations
            .get_result(&context.id, &actor.id, request_key)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(
                "Submission retry key was already used; choose a new submission attempt".into(),
            ));
        }
        if session.creator.id != actor.id {
            return Err(invalid("Only the session creator can submit this review"));
        }
        let input = Completion {
            outcome: Some(outcome.into()),
            body: Some(body.into()),
            submit: true,
        };
        if expected_head.is_empty()
            || session.commits.head.as_deref() != Some(expected_head)
            || intent
                .expected_base
                .as_ref()
                .is_some_and(|base| session.commits.base.as_ref() != Some(base))
        {
            return Err(Error::Conflict(
                "Expected PR base and head must match the reviewed comparison".into(),
            ));
        }
        if session.completion().is_some() {
            // Existing completion performs exact key/payload/actor validation.
            self.reviews
                .complete(&context.id, &actor, input, request_key, None, None)
                .await?;
            return self.reconcile(session_id, &actor).await;
        }
        if expected_head.is_empty() || session.commits.head.as_deref() != Some(expected_head) {
            return Err(Error::Conflict(
                "Expected PR head must match the reviewed session head".into(),
            ));
        }
        let github = self.get_provider().await?;
        let observed = self
            .validate_comparison(&github, &repo, pr, &session)
            .await?;
        let account = github.get_account_id().await?;
        if event != "COMMENT" && observed.author_account_id.as_deref() == Some(account.as_str()) {
            return Err(invalid(
                "GitHub does not allow approving or requesting changes on your own PR. Submit a comment review instead; your drafts are unchanged.",
            ));
        }
        let mut conversation = self.reviews.get_conversation(&context.id, true).await?;
        let mut comments: Vec<_> = conversation
            .comments
            .into_iter()
            .filter(|c| c.origin_session_id == Some(*session_id) && c.remote_presence != "missing")
            .collect();
        comments.sort_by_key(|c| (c.position, c.created_at.clone(), c.id));
        let mut prepared = Vec::new();
        for c in comments.iter().filter(|c| c.forge_comment_id.is_none()) {
            let payload = self
                .get_posting_payload(&session, c, &mut conversation.threads)
                .await?;
            prepared.push((c.clone(), payload, comment_key(session_id, &c.id)));
        }
        let frozen = self
            .reviews
            .prepare_submission(
                session_id,
                &actor,
                input,
                request_key,
                &prepared,
                intent.expected_version,
            )
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
                let rejected_event = matches!(error, Error::ReviewRejected(_));
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
                if rejected_event {
                    return self
                        .reviews
                        .resume_rejected_submission(session_id, &actor)
                        .await;
                }
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

    /// Prepare one private draft against the current PR while retaining its original anchor.
    async fn get_posting_payload(
        &self,
        session: &Session,
        comment: &Comment,
        threads: &mut [Thread],
    ) -> Result<String> {
        let mut body = render(comment);
        if comment.position == 0 {
            let thread = threads
                .iter_mut()
                .find(|t| t.id == comment.thread_id)
                .ok_or_else(|| invalid("Draft thread missing"))?;
            let placement = self.reviews.get_posting_anchor(session, thread).await?;
            if let (
                Anchor::Line {
                    path, start, end, ..
                },
                Anchor::File { .. },
            ) = (&thread.anchor, &placement)
            {
                body.push_str(&format!("\n\n> Original inline location: `{path}`, lines {start}-{end}. The selection was lost or ambiguous, so this feedback is retained at file level."));
                if let Some(evidence) = &thread.evidence {
                    body.push_str(&format!("\n\n```\n{}\n```", evidence.selected_text));
                }
            }
            thread.anchor = placement;
        }
        Ok(format!(
            "{}\n\n{}",
            body,
            comment_marker(session.id, comment.id)
        ))
    }

    async fn send_comment(
        &self,
        github: &Github,
        submission: &Submission<'_>,
        c: &Comment,
        pending: &serde_json::Value,
    ) -> Result<()> {
        let Submission {
            repo,
            pr,
            session,
            threads,
            account,
            ..
        } = *submission;
        self.validate_comparison(github, repo, pr, session).await?;
        let review_id = gh::opaque(pending, "id")?;
        let node_id = pending["node_id"]
            .as_str()
            .ok_or_else(|| invalid("Pending review receipt lacks node ID"))?;
        let thread = threads
            .iter()
            .find(|t| t.id == c.thread_id)
            .ok_or_else(|| invalid("Comment thread missing"))?;
        let body = self
            .reviews
            .get_comment(&c.context_id, &c.id)
            .await?
            .forge_body
            .unwrap_or_else(|| format!("{}\n\n{}", render(c), comment_marker(session.id, c.id)));
        let receipt = if c.position > 0 {
            if matches!(thread.anchor, Anchor::General) {
                return Err(invalid(
                    "GitHub replies require an existing review-comment thread",
                ));
            }
            let thread_id = thread
                .forge_thread_id
                .as_deref()
                .ok_or_else(|| invalid("Reply thread has no verified GitHub node ID"))?;
            let response = github.graphql(json!({"query":"mutation($input:AddPullRequestReviewThreadReplyInput!){addPullRequestReviewThreadReply(input:$input){comment{databaseId}}}","variables":{"input":{"pullRequestReviewId":node_id,"pullRequestReviewThreadId":thread_id,"body":body}}})).await?;
            PostReceipt {
                comment_id: gh::opaque(
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
                        comment_id: gh::opaque(&response, "id")?,
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
                        comment_id: gh::opaque(&t["comments"]["nodes"][0], "databaseId")?,
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
        Ok(())
    }

    async fn send(&self, github: &Github, submission: Submission<'_>) -> Result<String> {
        let Submission {
            repo,
            pr,
            session,
            comments,
            threads: _,
            account,
            event,
            body,
        } = submission;
        let marker = review_marker(session.id);
        let review_body = format!("{body}\n\n{marker}");
        let route = format!("{}/pulls/{pr}/reviews", repo.route());
        let review = if let Some(id) = &session.forge_review_id {
            self.get_pending(github, repo, pr, session, account)
                .await?
                .filter(|v| gh::opaque(v, "id").ok().as_ref() == Some(id))
                .ok_or_else(|| {
                    Error::Conflict(
                        "Pending review changed or disappeared; refresh before submitting".into(),
                    )
                })?
        } else {
            github
                .post(
                    &route,
                    &json!({"commit_id":session.commits.head,"body":review_body}),
                )
                .await?
        };
        let review_id = gh::opaque(&review, "id")?;
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
        for c in comments.iter().filter(|c| c.forge_comment_id.is_none()) {
            self.send_comment(github, &submission, c, &review).await?;
        }
        for kind in ["review_comment", "conversation_comment"] {
            let confirmed: Vec<_> = comments
                .iter()
                .filter(|comment| {
                    comment.forge_comment_id.is_some()
                        && comment.forge_object_kind.as_deref() == Some(kind)
                })
                .collect();
            if confirmed.is_empty() {
                continue;
            }
            let route = if kind == "review_comment" {
                format!("{route}/{review_id}/comments")
            } else {
                format!("{}/issues/{pr}/comments", repo.route())
            };
            let remote = gh::get_all(&github.client, &route).await?;
            for comment in confirmed {
                if !remote.iter().any(|value| {
                    gh::opaque(value, "id").ok() == comment.forge_comment_id
                        && value["body"].as_str() == comment.forge_body.as_deref()
                        && gh::opaque(&value["user"], "id").ok().as_deref() == Some(account)
                }) {
                    return Err(Error::Conflict(
                        "A synchronized comment changed on GitHub; refresh before submitting"
                            .into(),
                    ));
                }
            }
        }
        self.validate_comparison(github, repo, pr, session).await?;
        let response = github
            .post(
                &format!("{route}/{review_id}/events"),
                &json!({"event":event,"body":review_body}),
            )
            .await
            .map_err(|error| {
                if definite_rejection(&error) {
                    Error::ReviewRejected(error.to_string())
                } else {
                    error
                }
            })?;
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
        if session.state() == "active" && session.submission_status == "unsubmitted" {
            return Ok(session);
        }
        let github = self.get_provider().await?;
        let account = github.get_account_id().await?;
        let reviews = gh::get_all(
            &github.client,
            &format!("{}/pulls/{pr}/reviews", repo.route()),
        )
        .await?;
        let marker = review_marker(*session_id);
        let completion = session
            .completion()
            .ok_or_else(|| invalid("Session has no frozen submission"))?;
        let expected_body = format!("{}\n\n{marker}", completion.body.as_deref().unwrap_or(""));
        let matches: Vec<_> = reviews
            .iter()
            .filter(|v| {
                (v["body"].as_str() == Some(expected_body.as_str())
                    || (v["state"] == "PENDING"
                        && session.forge_review_id.as_deref()
                            == gh::opaque(v, "id").ok().as_deref()
                        && v["body"].as_str() == Some(marker.as_str())))
                    && gh::opaque(&v["user"], "id").ok().as_deref() == Some(account.as_str())
            })
            .collect();
        if matches.len() != 1 {
            return Err(Error::Conflict(
                "Submission cannot be reconciled uniquely; no write retried".into(),
            ));
        }
        let remote = matches[0];
        let id = gh::opaque(remote, "id")?;
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
        if (session.forge_review_id.as_ref() != Some(&id)
            && remote["commit_id"].as_str() != session.commits.head.as_deref())
            || (!pending && !state_matches(remote, final_event))
        {
            return Err(Error::Conflict("Review commit/outcome differs from the frozen submission; inspect it in GitHub before starting a new session".into()));
        }
        if pending && !["uncertain", "failed"].contains(&session.submission_status.as_str()) {
            return Err(Error::Conflict("Submission may still be running; wait and reconcile again. After a backend crash, inspect the pending review in GitHub before manual recovery".into()));
        }
        // Confirm exact staged payload plus account and a unique provider receipt.
        let conversation = self.reviews.get_conversation(&context.id, true).await?;
        let line = gh::get_all(
            &github.client,
            &format!("{}/pulls/{pr}/reviews/{id}/comments", repo.route()),
        )
        .await?;
        let general = gh::get_all(
            &github.client,
            &format!("{}/issues/{pr}/comments", repo.route()),
        )
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
                        && gh::opaque(&v["user"], "id").ok().as_deref() == Some(account.as_str())
                })
                .collect();
            if found.len() != 1 {
                return Err(Error::Conflict(
                    "Comment receipt is missing or ambiguous; no writes retried. Inspect the pending review in GitHub, reconcile accepted comments, or discard that pending review manually before starting a new session".into(),
                ));
            }
            let (v, kind) = found[0];
            let remote_id = gh::opaque(v, "id")?;
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
                        comment_id: gh::opaque(v, "id")?,
                        kind: kind.into(),
                        thread_id: None,
                        review_id: (kind == "review_comment").then(|| id.clone()),
                        posting_account_id: account.clone(),
                    },
                )
                .await?;
        }
        if pending && session.submission_status == "failed" {
            return self
                .reviews
                .resume_rejected_submission(session_id, &actor)
                .await;
        }
        let expected_status = if pending {
            self.validate_comparison(&github, &repo, pr, &session)
                .await?;
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
                    if definite_rejection(&error) {
                        return self
                            .reviews
                            .resume_rejected_submission(session_id, &actor)
                            .await;
                    }
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
        match self
            .change(
                session_id,
                actor,
                crate::models::ForgeChangeInput::Resolve {
                    thread_id: *thread_id,
                    resolved,
                },
                None,
            )
            .await?
        {
            crate::models::ForgeChangeObject::Thread(thread) => Ok(*thread),
            _ => Err(invalid("Unexpected remote resolution result")),
        }
    }

    async fn get_comment_node_id(&self, github: &Github, comment: &Comment) -> Result<String> {
        let conversation = self
            .reviews
            .get_conversation(&comment.context_id, true)
            .await?;
        let thread_id = conversation
            .threads
            .iter()
            .find(|t| t.id == comment.thread_id)
            .and_then(|t| t.forge_thread_id.as_deref())
            .ok_or_else(|| invalid("Review comment has no verified GitHub thread"))?;
        gh::get_review_comment(
            &github.client,
            thread_id,
            comment
                .forge_comment_id
                .as_deref()
                .ok_or_else(|| invalid("Missing GitHub comment receipt"))?,
        )
        .await?
        .and_then(|comment| comment["id"].as_str().map(str::to_owned))
        .ok_or_else(|| invalid("GitHub review comment missing from selected thread"))
    }

    pub async fn edit_comment(
        &self,
        session_id: &Uuid,
        comment_id: &Uuid,
        actor: &Actor,
        body: &str,
    ) -> Result<Comment> {
        match self
            .change(
                session_id,
                actor,
                crate::models::ForgeChangeInput::Edit {
                    comment_id: *comment_id,
                    body: body.into(),
                },
                None,
            )
            .await?
        {
            crate::models::ForgeChangeObject::Comment(comment) => Ok(*comment),
            _ => Err(invalid("Unexpected remote edit result")),
        }
    }

    pub async fn delete_comment(
        &self,
        session_id: &Uuid,
        comment_id: &Uuid,
        actor: &Actor,
    ) -> Result<()> {
        self.change(
            session_id,
            actor,
            crate::models::ForgeChangeInput::Delete {
                comment_id: *comment_id,
            },
            None,
        )
        .await?;
        Ok(())
    }

    pub fn new(reviews: ReviewService) -> Self {
        Self {
            reviews,
            provider: None,
            writes: Default::default(),
        }
    }

    pub fn with_provider(reviews: ReviewService, provider: Github) -> Self {
        Self {
            reviews,
            provider: Some(provider),
            writes: Default::default(),
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
    if (c.origin_session_id.is_some() && c.owner_id != Some(actor.id))
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
    if matches!(error, Error::ReviewRejected(_)) {
        return true;
    }
    let message = error.to_string();
    message
        .strip_prefix("GitHub HTTP ")
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|code| code.trim_end_matches(':').parse::<u16>().ok())
        .is_some_and(|status| (400..500).contains(&status) && status != 408)
}
