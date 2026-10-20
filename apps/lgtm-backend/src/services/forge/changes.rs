//! Remote object writes with durable intent and observation-based recovery.
use super::{ForgeService, check_owner, comment_marker, comment_route, definite_rejection, render};
use crate::{
    error::{Error, Result, invalid},
    forge::{Github, Repository},
    models::{
        Actor, Anchor, ForgeChange, ForgeChangeInput, ForgeChangeObject, MutationGuard, Session,
    },
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
        let _write = self.writes.lock().await;
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
        let (repo, pr) = Repository::from_context(&context.key)?;
        let github = self.get_provider().await?;
        let account = github.get_account_id().await?;
        let (saved, first) = if let Some(saved) = saved {
            (saved, false)
        } else {
            self.validate_comparison(&github, &repo, pr, &session)
                .await?;
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
            let rejected = match self.send_change(&github, &repo, pr, &session, &saved).await {
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
            return confirmed
                .result
                .ok_or_else(|| invalid("Remote result was not recorded"));
        }
        Err(Error::Conflict("Remote result is uncertain. Retry the same saved action to inspect its receipt; no write will be repeated".into()))
    }

    /// The outer error means no mutation was attempted; the inner result is the provider write.
    async fn send_change(
        &self,
        github: &Github,
        repo: &Repository,
        pr: i64,
        session: &Session,
        saved: &ForgeChange,
    ) -> Result<Result<()>> {
        self.validate_comparison(github, repo, pr, session).await?;
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
                updated.remote_presence = "present".into();
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
                let mut deleted = comment.clone();
                deleted.remote_presence = "missing".into();
                deleted.deleted = true;
                deleted.updated_at = now();
                Ok(Some(ForgeChangeObject::Comment(deleted)))
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
                if let Some(observation) = &updated.forge_observation {
                    updated.outdated = observation.outdated;
                }
                Ok(Some(ForgeChangeObject::Thread(updated)))
            }
            _ => Err(invalid("Saved remote intent has an incompatible object")),
        }
    }
}
