//! Remote object writes with durable intent and observation-based recovery.
use super::{ForgeService, check_owner, comment_marker, comment_route, definite_rejection, render};
use crate::extensions::gh;
use crate::{
    error::{Error, Result, invalid},
    forge::{Github, Repository},
    models::{Actor, Anchor, ForgeChange, ForgeChangeInput, ForgeChangeObject, MutationGuard},
    utils::{ForgeSnapshot, now},
};
use serde_json::json;
use uuid::Uuid;

impl ForgeService {
    pub async fn change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: ForgeChangeInput,
        guard: Option<&MutationGuard>,
    ) -> Result<ForgeChangeObject> {
        let actor = self.reviews.auth().validate_requester(actor).await?;
        let session = self.reviews.get_session(session_id).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let local_replay = self
            .reviews
            .database()
            .mutations
            .get_result(
                &context.id,
                &actor.id,
                guard.map(|guard| guard.request_key.as_str()).unwrap_or(""),
            )
            .await?
            .is_some_and(|receipt| {
                ["edit_comment", "delete_comment", "resolve"].contains(&receipt.method.as_str())
            });
        if !context.key.is_forge_backed() || local_replay {
            return match &input {
                ForgeChangeInput::Edit { comment_id, body } => {
                    Ok(ForgeChangeObject::Comment(Box::new(
                        self.reviews
                            .edit_comment(session_id, comment_id, &actor, body, guard)
                            .await?,
                    )))
                }
                ForgeChangeInput::Resolve {
                    thread_id,
                    resolved,
                } => Ok(ForgeChangeObject::Thread(Box::new(
                    self.reviews
                        .resolve(session_id, thread_id, &actor, *resolved, guard)
                        .await?,
                ))),
                ForgeChangeInput::Delete { comment_id } => {
                    self.reviews
                        .delete_comment(session_id, comment_id, &actor, guard)
                        .await?;
                    Ok(ForgeChangeObject::Deleted {
                        comment_id: *comment_id,
                    })
                }
            };
        }
        let owned_guard = guard.cloned().unwrap_or_else(|| {
            MutationGuard::new(Some(context.revision), Uuid::new_v4().to_string())
        });
        let saved = if context.key.is_forge_backed() {
            self.reviews
                .get_forge_change(session_id, &actor, &input, &owned_guard)
                .await?
        } else {
            None
        };
        if let Some(saved) = &saved {
            if let Some(result) = &saved.result {
                return Ok(result.clone());
            }
            if let Some(rejection) = &saved.rejection {
                return Err(Error::Conflict(rejection.clone()));
            }
        }
        if saved.is_none() {
            match &input {
                ForgeChangeInput::Edit { comment_id, body } => {
                    let comment = self.reviews.get_comment(&context.id, comment_id).await?;
                    if comment.forge_comment_id.is_none() {
                        return Ok(ForgeChangeObject::Comment(Box::new(
                            self.reviews
                                .edit_comment(session_id, comment_id, &actor, body, guard)
                                .await?,
                        )));
                    }
                }
                ForgeChangeInput::Delete { comment_id } => {
                    let comment = self.reviews.get_comment(&context.id, comment_id).await?;
                    if comment.forge_comment_id.is_none() {
                        self.reviews
                            .delete_comment(session_id, comment_id, &actor, guard)
                            .await?;
                        return Ok(ForgeChangeObject::Deleted {
                            comment_id: *comment_id,
                        });
                    }
                }
                ForgeChangeInput::Resolve {
                    thread_id,
                    resolved,
                } => {
                    let conversation = self.reviews.get_conversation(&context.id, true).await?;
                    let thread = conversation
                        .threads
                        .into_iter()
                        .find(|t| t.id == *thread_id)
                        .ok_or_else(|| Error::NotFound("Thread".into()))?;
                    if !conversation.comments.iter().any(|comment| {
                        comment.thread_id == *thread_id && comment.visibility != "draft"
                    }) {
                        return Err(invalid(
                            "Publish draft comments before resolving their thread",
                        ));
                    }
                    if thread.forge_thread_id.is_none() {
                        return Ok(ForgeChangeObject::Thread(Box::new(
                            self.reviews
                                .resolve(session_id, thread_id, &actor, *resolved, guard)
                                .await?,
                        )));
                    }
                }
            }
        }
        if matches!(
            input,
            ForgeChangeInput::Resolve { .. } | ForgeChangeInput::Delete { .. }
        ) {
            return self
                .change_target(&session.id, &actor, &input, &owned_guard, saved)
                .await;
        }
        let _write = self.writes.lock().await;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        let account = github.get_account_id().await?;
        let (saved, first) = if let Some(saved) = saved {
            (saved, false)
        } else {
            let original = match &input {
                ForgeChangeInput::Edit { comment_id, .. }
                | ForgeChangeInput::Delete { comment_id } => {
                    let comment = self.reviews.get_comment(&context.id, comment_id).await?;
                    check_owner(&comment, &actor, &account)?;
                    if matches!(input, ForgeChangeInput::Delete { .. })
                        && comment.forge_object_kind.as_deref() == Some("review")
                    {
                        return Err(invalid(
                            "Submitted reviews cannot be deleted through comment deletion",
                        ));
                    }
                    ForgeChangeObject::Comment(Box::new(comment))
                }
                ForgeChangeInput::Resolve { thread_id, .. } => {
                    let conversation = self.reviews.get_conversation(&context.id, true).await?;
                    let thread = conversation
                        .threads
                        .into_iter()
                        .find(|t| t.id == *thread_id)
                        .ok_or_else(|| Error::NotFound("Thread".into()))?;
                    if matches!(thread.anchor, Anchor::General) {
                        return Err(invalid("GitHub only supports resolving review threads"));
                    }
                    ForgeChangeObject::Thread(Box::new(thread))
                }
            };
            let snapshot = github.get_snapshot(&repo, pr).await?;
            match &original {
                ForgeChangeObject::Comment(comment) => {
                    let remote = snapshot.threads.iter().flat_map(|t| &t.comments).find(|c| {
                        Some(&c.id) == comment.forge_comment_id.as_ref()
                            && Some(&c.kind) == comment.forge_object_kind.as_ref()
                    });
                    if remote.is_none_or(|remote| {
                        Some(remote.body.as_str()) != comment.forge_body.as_deref()
                    }) {
                        return Err(Error::Conflict(
                            "Comment changed on GitHub; refresh before editing".into(),
                        ));
                    }
                }
                ForgeChangeObject::Thread(thread) => {
                    let remote = snapshot
                        .threads
                        .iter()
                        .find(|t| Some(&t.id) == thread.forge_thread_id.as_ref());
                    if remote.is_none_or(|remote| remote.resolved != (thread.state == "resolved")) {
                        return Err(Error::Conflict(
                            "Thread changed on GitHub; refresh before resolving".into(),
                        ));
                    }
                }
                ForgeChangeObject::Deleted { .. } => {
                    return Err(invalid("A deleted local object cannot be a remote source"));
                }
            }
            self.reviews
                .prepare_forge_change(session_id, &actor, &input, &owned_guard)
                .await?
        };
        if first {
            let rejected = match self.send_change(&github, &repo, pr, &saved).await {
                Err(error) => Some(error), // Failed before any provider mutation was attempted.
                Ok(Err(error)) if definite_rejection(&error) => Some(error),
                _ => None,
            };
            if let Some(error) = rejected {
                self.reviews
                    .finish_forge_change(
                        session_id,
                        &actor,
                        &input,
                        &owned_guard,
                        None,
                        Some(error.to_string()),
                    )
                    .await?;
                return Err(error);
            }
        }
        let snapshot = github.get_snapshot(&repo, pr).await?;
        if let Some(result) = Self::get_changed_object(&saved, &snapshot, &account)? {
            let confirmed = self
                .reviews
                .finish_forge_change(session_id, &actor, &input, &owned_guard, Some(result), None)
                .await?;
            if session.state() == "active"
                && (session.commits.head.as_deref() != Some(snapshot.head.as_str())
                    || session.commits.base != snapshot.base)
                && let Err(error) = self.refresh_context(&context.id, None).await
            {
                // The object mutation is already confirmed. Preserve its receipt and
                // let the next explicit refresh retry this read-only synchronization.
                tracing::warn!(%error, "PR comparison refresh after confirmed change failed");
            }
            return confirmed
                .result
                .ok_or_else(|| invalid("Remote result was not recorded"));
        }
        Err(Error::Conflict("Remote result is uncertain. Retry the same saved action to inspect its receipt; no write will be repeated".into()))
    }

    /// Confirm resolve/delete from their own provider response. Recovery observes only that object.
    /// The database ledger serializes conflicting writes to one object without a global network lock.
    async fn change_target(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
        saved: Option<ForgeChange>,
    ) -> Result<ForgeChangeObject> {
        let github = self.get_provider().await?;
        let (saved, first) = match saved {
            Some(saved) => (saved, false),
            None => {
                self.reviews
                    .prepare_forge_change(session_id, actor, input, guard)
                    .await?
            }
        };
        if let Some(result) = saved.result {
            return Ok(result);
        }
        if let Some(rejection) = saved.rejection {
            return Err(Error::Conflict(rejection));
        }
        let session = self.reviews.get_session(session_id).await?;
        let context = self.reviews.get_context_by_id(&session.context_id).await?;
        let (repo, pr) = Repository::from_context(&context.key)?;
        let mut attempted = false;
        let result: Result<Option<ForgeChangeObject>> = async {
            match (&saved.input, &saved.original) {
                (ForgeChangeInput::Resolve { resolved, .. }, ForgeChangeObject::Thread(thread)) => {
                    let id = thread.forge_thread_id.as_deref()
                        .ok_or_else(|| invalid("Thread has no GitHub receipt"))?;
                    let confirmed = if first {
                        let mutation = if *resolved { "resolveReviewThread" } else { "unresolveReviewThread" };
                        attempted = true;
                        let response = github.graphql(json!({
                            "query": format!("mutation($id:ID!){{{mutation}(input:{{threadId:$id}}){{thread{{id isResolved}}}}}}"),
                            "variables": {"id": id}
                        })).await?;
                        response[mutation]["thread"]["id"] == id
                            && response[mutation]["thread"]["isResolved"].as_bool() == Some(*resolved)
                    } else {
                        gh::get_thread_resolution(&github.client, id).await? == Some(*resolved)
                    };
                    if !confirmed {
                        return Ok(None);
                    }
                    let mut updated = thread.clone();
                    updated.state = if *resolved { "resolved" } else { "open" }.into();
                    updated.updated_at = now();
                    updated.resolved_at = None;
                    updated.resolved_by = None;
                    updated.updated_by = None;
                    Ok(Some(ForgeChangeObject::Thread(updated)))
                }
                (ForgeChangeInput::Delete { comment_id }, ForgeChangeObject::Comment(comment)) => {
                    if comment.origin_session_id.is_some() && comment.owner_id != Some(actor.id) {
                        return Err(invalid("Only the draft owner may delete this feedback"));
                    }
                    if comment.forge_object_kind.as_deref() == Some("review") {
                        return Err(invalid("Submitted reviews cannot be deleted as comments"));
                    }
                    let route = comment_route(&repo, pr, comment)?;
                    if comment.forge_object_kind.as_deref() == Some("review_comment") {
                        let thread = self.reviews.database().comments
                            .get_thread(&context.id, &comment.thread_id).await?
                            .ok_or_else(|| invalid("Comment thread missing"))?;
                        let thread_id = thread.forge_thread_id.as_deref()
                            .ok_or_else(|| invalid("Comment thread has no GitHub receipt"))?;
                        let remote = gh::get_review_comment(&github.client,
                            thread_id,
                            comment.forge_comment_id.as_deref()
                                .ok_or_else(|| invalid("Comment has no GitHub receipt"))?,
                        ).await?;
                        if let Some(remote) = remote {
                            if !first {
                                return Ok(None);
                            }
                            if remote["viewerDidAuthor"] != true
                                || remote["body"].as_str() != comment.forge_body.as_deref()
                            {
                                return Err(invalid("Comment changed or belongs to another GitHub account; refresh before deleting"));
                            }
                            let node = remote["id"].as_str()
                                .ok_or_else(|| invalid("Comment node ID missing"))?;
                            attempted = true;
                            let response = github.graphql(json!({
                                "query": "mutation($id:ID!){deletePullRequestReviewComment(input:{id:$id}){clientMutationId}}",
                                "variables": {"id": node}
                            })).await?;
                            if !response.get("deletePullRequestReviewComment").is_some_and(serde_json::Value::is_object) {
                                return Ok(None);
                            }
                        } else {
                            // A hidden pending comment must not look deleted after an account switch.
                            check_owner(comment, actor, &github.get_account_id().await?)?;
                            github.get_pull_request(&repo, pr).await?;
                        }
                    } else {
                        // REST comments have no viewer-author field; verify the posting account.
                        check_owner(comment, actor, &github.get_account_id().await?)?;
                        if first {
                            attempted = true;
                            github.delete(&route).await?;
                        } else {
                            match github.get(&route, None).await {
                                Ok(_) => return Ok(None),
                                Err(error) if error.to_string().starts_with("GitHub HTTP 404 ") => {
                                    github.get_pull_request(&repo, pr).await?;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                    }
                    Ok(Some(ForgeChangeObject::Deleted { comment_id: *comment_id }))
                }
                _ => Err(invalid("Unsupported targeted change")),
            }
        }.await;
        match result {
            Ok(Some(result)) => self.reviews
                .finish_forge_change(session_id, actor, input, guard, Some(result), None)
                .await?.result.ok_or_else(|| invalid("Mutation receipt was not persisted")),
            Err(error) if first && (!attempted || definite_rejection(&error)) => {
                let message = format!("Request rejected: {error}");
                self.reviews.finish_forge_change(session_id, actor, input, guard, None, Some(message.clone())).await?;
                Err(Error::Conflict(message))
            }
            Err(error) => Err(Error::Conflict(format!("Remote result is uncertain. Retry the same action to inspect its receipt; no write will be repeated. {error}"))),
            Ok(None) => Err(Error::Conflict("Remote result is uncertain. Retry the same action to inspect its receipt; no write will be repeated".into())),
        }
    }

    /// The outer error means no mutation was attempted; the inner result is the provider write.
    async fn send_change(
        &self,
        github: &Github,
        repo: &Repository,
        pr: i64,
        saved: &ForgeChange,
    ) -> Result<Result<()>> {
        let result = match (&saved.input, &saved.original) {
            (ForgeChangeInput::Edit { body, .. }, ForgeChangeObject::Comment(comment)) => {
                let rendered = Self::get_change_body(comment, body);
                if comment.forge_object_kind.as_deref() == Some("review_comment") {
                    let node = self.get_comment_node_id(github, comment).await?;
                    github.graphql(json!({"query":"mutation($id:ID!,$body:String!){updatePullRequestReviewComment(input:{pullRequestReviewCommentId:$id,body:$body}){pullRequestReviewComment{id body}}}","variables":{"id":node,"body":rendered}})).await.map(|_| ())
                } else {
                    github
                        .patch(
                            &comment_route(repo, pr, comment)?,
                            &json!({"body":rendered}),
                        )
                        .await
                        .map(|_| ())
                }
            }
            (ForgeChangeInput::Delete { .. }, ForgeChangeObject::Comment(comment)) => {
                if comment.forge_object_kind.as_deref() == Some("review_comment") {
                    let node = self.get_comment_node_id(github, comment).await?;
                    github.graphql(json!({"query":"mutation($id:ID!){deletePullRequestReviewComment(input:{id:$id}){clientMutationId}}","variables":{"id":node}})).await.map(|_| ())
                } else {
                    github.delete(&comment_route(repo, pr, comment)?).await
                }
            }
            (ForgeChangeInput::Resolve { resolved, .. }, ForgeChangeObject::Thread(thread)) => {
                let mutation = if *resolved {
                    "resolveReviewThread"
                } else {
                    "unresolveReviewThread"
                };
                let id = thread
                    .forge_thread_id
                    .as_deref()
                    .ok_or_else(|| invalid("Remote thread ID missing"))?;
                github.graphql(json!({"query":format!("mutation($id:ID!){{{mutation}(input:{{threadId:$id}}){{thread{{id isResolved}}}}}}"),"variables":{"id":id}})).await.map(|_| ())
            }
            _ => return Err(invalid("Saved remote intent has an incompatible object")),
        };
        Ok(result)
    }

    fn get_change_body(comment: &crate::models::Comment, body: &str) -> String {
        let mut updated = comment.clone();
        updated.body = body.into();
        if let Some(origin) = updated.origin_session_id {
            format!(
                "{}\n\n{}",
                render(&updated),
                comment_marker(origin, updated.id)
            )
        } else {
            body.into()
        }
    }

    fn get_changed_object(
        saved: &ForgeChange,
        snapshot: &ForgeSnapshot,
        account: &str,
    ) -> Result<Option<ForgeChangeObject>> {
        if !snapshot.complete {
            return Ok(None);
        }
        match (&saved.input, &saved.original) {
            (ForgeChangeInput::Edit { body, .. }, ForgeChangeObject::Comment(comment)) => {
                let mut updated = comment.as_ref().clone();
                updated.body = body.clone();
                let rendered = Self::get_change_body(comment, body);
                let remote = snapshot.threads.iter().flat_map(|t| &t.comments).find(|c| {
                    Some(&c.id) == comment.forge_comment_id.as_ref()
                        && Some(&c.kind) == comment.forge_object_kind.as_ref()
                });
                let Some(remote) =
                    remote.filter(|c| c.body == rendered && c.author.account_id == account)
                else {
                    return Ok(None);
                };
                updated.forge_body = Some(rendered);
                updated.forge_updated_at = Some(remote.updated_at.clone());
                updated.updated_at = now();

                updated.visibility = if remote.pending { "draft" } else { "published" }.into();
                Ok(Some(ForgeChangeObject::Comment(Box::new(updated))))
            }
            (ForgeChangeInput::Delete { .. }, ForgeChangeObject::Comment(comment)) => {
                if snapshot.threads.iter().flat_map(|t| &t.comments).any(|c| {
                    Some(&c.id) == comment.forge_comment_id.as_ref()
                        && Some(&c.kind) == comment.forge_object_kind.as_ref()
                }) {
                    return Ok(None);
                }
                Ok(Some(ForgeChangeObject::Deleted {
                    comment_id: comment.id,
                }))
            }
            (ForgeChangeInput::Resolve { resolved, .. }, ForgeChangeObject::Thread(thread)) => {
                let Some(remote) = snapshot.threads.iter().find(|t| {
                    Some(&t.id) == thread.forge_thread_id.as_ref() && t.resolved == *resolved
                }) else {
                    return Ok(None);
                };
                let mut updated = thread.clone();
                updated.state = if *resolved { "resolved" } else { "open" }.into();
                updated.updated_at = now();
                updated.resolved_at = None;
                updated.resolved_by = None;
                updated.updated_by = None;
                updated.forge_observation = remote.observation.clone();
                Ok(Some(ForgeChangeObject::Thread(updated)))
            }
            _ => Err(invalid("Saved remote intent has an incompatible object")),
        }
    }
}
