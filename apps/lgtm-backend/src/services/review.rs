//! Review application actions over an injected database client and request tracing context.
use super::super::database as db;
use super::super::{
    error::{Error, Result, invalid},
    models::*,
};

use super::AuthService;
use super::actor::{get_actor, load_actors};
use crate::forge::models::{ForgeSnapshot, PostReceipt};
use crate::models::{ForgeChange, ForgeChangeInput, ForgeChangeObject};
use crate::utils::{now, within};
use db::Database;
use db::models::*;
use sea_orm::{ConnectionTrait, TransactionTrait};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone)]
pub struct ReviewService {
    database: Database,
    auth: AuthService,
    sources: super::diff::DiffService,
}

/// Remote acknowledgments contain no copy of the comment, thread, or request body.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ChangeReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    rejection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_remote: Option<(String, String)>,
}

impl ReviewService {
    // Public ------------------------------------------------------------------

    // Construction and configuration ------------------------------------------

    pub fn new(database: Database, auth: AuthService) -> Self {
        Self {
            database,
            auth,
            sources: super::diff::DiffService::default(),
        }
    }

    pub fn auth(&self) -> &AuthService {
        &self.auth
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn with_sources(mut self, sources: super::diff::DiffService) -> Self {
        self.sources = sources;
        self
    }

    // Contexts ----------------------------------------------------------------

    /// Associate an optional checkout without changing PR identity or loading file content.
    pub async fn associate_checkout(&self, context_id: &uuid::Uuid, path: &str) -> Result<Context> {
        let context = self.get_context_by_id(context_id).await?;
        if !context.key.is_forge_backed() {
            return Err(invalid(
                "Checkout association is only needed for a PR target",
            ));
        }
        let (root, _, _) = super::workspace::get_git_repository(path).await?;
        self.ensure_context(context.key, Some(&root)).await
    }

    pub async fn get_context(&self, key: &ContextKey) -> Result<Option<Context>> {
        key.validate()?;
        match self
            .database
            .review_contexts
            .get_by_key(key.as_database_key())
            .await?
        {
            Some(row) => Ok(Some(
                Self::context_from_db_model(row, &self.database).await?,
            )),
            None => Ok(None),
        }
    }

    pub async fn get_context_by_id(&self, id: &uuid::Uuid) -> Result<Context> {
        self.load_context(&self.database, id).await
    }

    pub async fn ensure_context(&self, key: ContextKey, path: Option<&str>) -> Result<Context> {
        key.validate()?;
        if path.is_some_and(|p| !std::path::Path::new(p).is_absolute()) {
            return Err(invalid("Workspace path must be canonical and absolute"));
        }
        let tx = self.database.begin().await?;
        let seed = Context {
            id: uuid::Uuid::new_v4(),
            key: key.clone(),
            paths: vec![],
            current_notes_path: None,
            revision: 0,
            last_refreshed_at: None,
            refreshed_head: None,
            refreshed_base: None,
            refresh_status: "never".into(),
            forge_state: None,
            refresh_error: None,
        };
        tx.review_contexts
            .insert(&review_contexts::Model::from(&seed))
            .await?;
        let row = tx
            .review_contexts
            .get_by_key(key.as_database_key())
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?;
        if let Some(path) = path {
            tx.workspace
                .insert_path(&workspace_paths::Model {
                    context_id: row.id,
                    path: path.into(),
                })
                .await?;
        }
        let context = Self::context_from_db_model(row, &tx).await?;
        tx.commit().await?;
        Ok(context)
    }

    /// A notes read returns no storage when the file has no discussion.
    pub async fn get_scope_context(&self, scope: &ReviewScope) -> Result<Option<Context>> {
        match scope {
            ReviewScope::Review { review_id } => Ok(Some(
                self.get_context_by_id(&self.get_session(review_id).await?.context_id)
                    .await?,
            )),
            ReviewScope::Notes { path } => {
                let path = Self::get_notes_path(path)?;
                match self
                    .database
                    .review_contexts
                    .get_notes_for_path(&path)
                    .await?
                {
                    Some(row) => Ok(Some(
                        Self::context_from_db_model(row, &self.database).await?,
                    )),
                    None => Ok(None),
                }
            }
        }
    }

    /// Resolve a consumer target into internal storage. The target location is always explicit.
    pub async fn resolve_target(&self, target: &ReviewTarget) -> Result<Context> {
        match target {
            ReviewTarget::Directory { path } => {
                let path = std::fs::canonicalize(path)?;
                if !path.is_dir() {
                    return Err(invalid("Directory review requires a directory"));
                }
                let path = path.to_string_lossy().into_owned();
                if let Some(context) = self
                    .get_scope_context(&ReviewScope::Notes { path: path.clone() })
                    .await?
                {
                    return Ok(context);
                }
                self.ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Notes { path },
                    },
                    None,
                )
                .await
            }
            ReviewTarget::LocalGit {
                path,
                branch: expected_branch,
            } => {
                let (root, git_dir, branch) = super::workspace::get_git_location(path).await?;
                if expected_branch
                    .as_ref()
                    .is_some_and(|expected| expected != &branch)
                {
                    return Err(Error::Conflict("The selected branch changed".into()));
                }
                self.ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Branch { git_dir, branch },
                    },
                    Some(&root),
                )
                .await
            }
            ReviewTarget::PullRequest {
                repository_url,
                number,
            } => {
                self.ensure_context(
                    ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse(repository_url)?,
                        number: *number,
                    },
                    None,
                )
                .await
            }
        }
    }

    // Reviews and sessions ----------------------------------------------------

    /// Get the immutable revision accepted for a retry key, even after later mutations.
    pub async fn get_accepted_version(
        &self,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        key: &str,
    ) -> Result<i64> {
        self.database
            .mutations
            .get_result(context, actor, key)
            .await?
            .map(|r| r.version)
            .ok_or_else(|| Error::NotFound("Accepted mutation receipt".into()))
    }

    pub async fn get_session(&self, id: &uuid::Uuid) -> Result<Session> {
        let row = self
            .database
            .sessions
            .get_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id}")))?;
        Self::session_from_db_model(row, &self.database).await
    }

    pub async fn get_selected_session(
        &self,
        context: &uuid::Uuid,
        id: Option<&uuid::Uuid>,
        active_only: bool,
    ) -> Result<Session> {
        if let Some(id) = id {
            let s = self.get_session(id).await?;
            if s.context_id != *context {
                return Err(invalid("Session belongs to another context"));
            }
            return Ok(s);
        }
        let mut rows = Self::sessions_from_db_models(
            self.database
                .sessions
                .get_all(Some(context), active_only)
                .await?,
            &self.database,
        )
        .await?;
        rows.sort_by(|a, b| {
            (
                a.state() != "active",
                std::cmp::Reverse(&a.created_at),
                &a.id,
            )
                .cmp(&(
                    b.state() != "active",
                    std::cmp::Reverse(&b.created_at),
                    &b.id,
                ))
        });
        rows.into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("Session; run lgtm session start".into()))
    }

    pub async fn get_sessions(&self, filter: &SessionFilter) -> Result<Vec<(Context, Session)>> {
        let mut result = vec![];
        for session in Self::sessions_from_db_models(
            self.database
                .sessions
                .get_all(None, !filter.completed)
                .await?,
            &self.database,
        )
        .await?
        {
            let context = self.get_context_by_id(&session.context_id).await?;
            if filter
                .git_dir
                .as_deref()
                .is_some_and(|r| context.key.git_dir() != Some(r))
            {
                continue;
            }
            if let Some(root) = &filter.path {
                let matches = match &context.key {
                    ContextKey::Local {
                        location: LocalLocation::Notes { path },
                    } => within(path, root),
                    _ => context
                        .paths
                        .iter()
                        .any(|p| within(p, root) || within(root, p)),
                };
                if !matches {
                    continue;
                }
            }
            result.push((context, session));
        }
        result.sort_by(|a, b| {
            (&a.0.id, &a.1.created_at, &a.1.id).cmp(&(&b.0.id, &b.1.created_at, &b.1.id))
        });
        Ok(result)
    }

    /// Read a session-backed public review with a consistent conversation version.
    pub async fn get_review(&self, id: &uuid::Uuid) -> Result<Review> {
        let seed = self.get_session(id).await?;
        let context = self.get_context_by_id(&seed.context_id).await?;
        let session = self.get_session(id).await?;
        let target = match &context.key {
            ContextKey::Local {
                location: LocalLocation::Notes { path },
            } => ReviewTarget::Directory {
                path: context
                    .current_notes_path
                    .clone()
                    .unwrap_or_else(|| path.clone()),
            },
            ContextKey::Local {
                location: LocalLocation::Branch { git_dir, branch },
            } => ReviewTarget::LocalGit {
                branch: Some(branch.clone()),
                path: context
                    .paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| git_dir.clone()),
            },
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => ReviewTarget::PullRequest {
                repository_url: repository_url.as_str().into(),
                number: *number,
            },
        };
        let comparison_kind = session.comparison_kind;
        let comparison = match comparison_kind {
            ComparisonKind::Working => session
                .commits
                .base
                .clone()
                .map(|base_commit| ReviewComparison::Working { base_commit }),
            ComparisonKind::Staged => session
                .commits
                .base
                .clone()
                .map(|base_commit| ReviewComparison::Staged { base_commit }),
            ComparisonKind::Directory => Some(ReviewComparison::Directory),
            ComparisonKind::Branch | ComparisonKind::PullRequest => session
                .commits
                .base
                .clone()
                .zip(session.commits.head.clone())
                .map(|(base_commit, head_commit)| {
                    if comparison_kind == ComparisonKind::Branch {
                        ReviewComparison::Branch {
                            base_commit,
                            head_commit,
                        }
                    } else {
                        ReviewComparison::PullRequest {
                            base_commit,
                            head_commit,
                        }
                    }
                }),
            ComparisonKind::Notes => None,
        };
        if self.get_context_by_id(&context.id).await?.revision != context.revision {
            return Err(Error::Conflict(
                "Review changed during the read; retry".into(),
            ));
        }
        Ok(Review {
            target,
            comparison,
            observed_comparison: if context.key.is_forge_backed() {
                context
                    .refreshed_base
                    .clone()
                    .zip(context.refreshed_head.clone())
                    .map(|(base_commit, head_commit)| ReviewComparison::PullRequest {
                        base_commit,
                        head_commit,
                    })
            } else {
                None
            },
            session,
            version: context.revision,
            forge_state: context.forge_state,
        })
    }

    /// Read a bounded page of saved reviews without creating records or refreshing a forge.
    pub async fn get_reviews(
        &self,
        target: Option<&ReviewTarget>,
        state: Option<&str>,
        after: Option<uuid::Uuid>,
        limit: u64,
    ) -> Result<Vec<Review>> {
        let state = match state {
            Some("active") => Some(SessionState::Active),
            Some("completed") => Some(SessionState::Completed),
            None => None,
            _ => return Err(invalid("Expected active or completed review state")),
        };
        let mut context_id = None;
        let mut workspace = None;
        let mut branch = None;
        match target {
            Some(ReviewTarget::LocalGit {
                path,
                branch: selected_branch,
            }) => {
                let path = std::path::Path::new(path);
                if !path.is_absolute() {
                    return Err(invalid("Review listing requires an absolute location"));
                }
                workspace = Some(
                    path.canonicalize()
                        .unwrap_or_else(|_| path.to_path_buf())
                        .to_string_lossy()
                        .into_owned(),
                );
                branch = selected_branch.as_deref();
            }
            Some(ReviewTarget::Directory { path }) => {
                let Some(context) = self
                    .get_scope_context(&ReviewScope::Notes { path: path.clone() })
                    .await?
                else {
                    return Ok(vec![]);
                };
                context_id = Some(context.id);
            }
            Some(ReviewTarget::PullRequest {
                repository_url,
                number,
            }) => {
                let key = ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse(repository_url)?,
                    number: *number,
                };
                let Some(context) = self.get_context(&key).await? else {
                    return Ok(vec![]);
                };
                context_id = Some(context.id);
            }
            None => {}
        }
        let rows = self
            .database
            .sessions
            .get_review_page(
                context_id,
                workspace.as_deref(),
                branch,
                state,
                after,
                limit,
            )
            .await?;
        let mut reviews = Vec::with_capacity(rows.len());
        for row in rows {
            reviews.push(self.get_review(&row.id).await?);
        }
        Ok(reviews)
    }

    /// Recover an accepted start before a PR refresh or source check can fail.
    pub async fn get_started_session(
        &self,
        context: &Context,
        actor: &Actor,
        supplied: Option<CommitContext>,
        selection: &StartOptions,
        guard: &MutationGuard,
    ) -> Result<Option<Session>> {
        let Some(receipt) = self
            .database
            .mutations
            .get_result(&context.id, &actor.id, &guard.request_key)
            .await?
        else {
            return Ok(None);
        };
        if receipt.method != "start_session" {
            return Err(Error::Conflict(
                "Retry key belongs to another action".into(),
            ));
        }
        let saved: Session = serde_json::from_str(&receipt.result_json)?;
        let commits = supplied.unwrap_or_else(|| saved.commits.clone());
        let guard = guard.bind(&(&context.id, &commits, selection))?;
        if receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict(
                "Retry key belongs to another review selection".into(),
            ));
        }
        Ok(Some(saved))
    }

    pub async fn start_session(
        &self,
        context: &Context,
        actor: &Actor,
        commits: CommitContext,
        selection: impl Into<StartOptions> + Send,
        guard: Option<&MutationGuard>,
    ) -> Result<Session> {
        let selection = selection.into();
        let comparison_kind = selection.comparison_kind;
        if selection.replace_local && (context.key.is_forge_backed() || guard.is_none()) {
            return Err(invalid(
                "Replacing an attempt requires a local target and explicit retry key",
            ));
        }
        if selection.accept_pr_comparison && (!context.key.is_forge_backed() || guard.is_none()) {
            return Err(invalid(
                "PR comparison acceptance requires a PR target and retry identity",
            ));
        }
        let guard = guard
            .map(|g| g.bind(&(&context.id, &commits, &selection)))
            .transpose()?;
        let guard = guard.as_ref();
        tracing::debug!("Review action: start_session");
        let actor = &self.auth.validate_requester(actor).await?;
        let tx = self.database.begin().await?;
        if comparison_kind == ComparisonKind::Notes && guard.is_none() {
            if !tx.review_contexts.lock_for_write(&context.id).await? {
                return Err(Error::NotFound("Notes context".into()));
            }
        } else {
            if let Some(saved) = self
                .begin_mutation::<Session>(
                    &tx,
                    &context.id,
                    &actor.id,
                    "start_session",
                    guard,
                    false,
                )
                .await?
            {
                tx.commit().await?;
                return Ok(saved);
            }
        }
        let context = self.load_context(&tx, &context.id).await?;
        let row = tx.sessions.get_active(&context.id).await?;
        if let Some(expected) = selection.expected_active_id
            && row.as_ref().is_none_or(|row| row.id != expected)
        {
            return Err(Error::Conflict(
                "The selected active attempt changed; review the current selection".into(),
            ));
        }
        if let Some(row) = row {
            let mut saved = Self::session_from_db_model(row, &tx).await?;
            let effective_kind = saved.comparison_kind;
            if selection.replace_local {
                for mut comment in Self::comments_from_db_models(
                    tx.comments.get_session_draft_comments(&saved.id).await?,
                    &tx,
                )
                .await?
                {
                    comment.visibility = "published".into();
                    comment.updated_at = now();
                    tx.comments
                        .update_comment(&comments::Model::from(&comment))
                        .await?;
                }
                let guard =
                    guard.ok_or_else(|| invalid("Replacement requires its retry identity"))?;
                saved.complete(
                    actor.clone(),
                    if context.key.is_notes() {
                        "saved"
                    } else {
                        "ready-to-address"
                    }
                    .into(),
                    None,
                    format!("replace:{}:{}", actor.id, guard.request_key),
                    guard.payload_hash.clone(),
                )?;
                if !tx.sessions.complete(&sessions::Model::from(&saved)).await? {
                    return Err(Error::Conflict("Selected local attempt changed".into()));
                }
            } else {
                if effective_kind != comparison_kind || saved.commits != commits {
                    return Err(Error::Conflict("Another comparison is active; resume it or explicitly replace the local attempt".into()));
                }
                Self::record_mutation(&tx, &context.id, &actor.id, "start_session", guard, &saved)
                    .await?;
                tx.commit().await?;
                return Ok(saved);
            }
        }
        if context.key.is_forge_backed() {
            let fresh = self.load_context(&tx, &context.id).await?;
            if !["live", "stub"].contains(&fresh.refresh_status.as_str())
                || fresh.last_refreshed_at.is_none()
            {
                return Err(invalid(
                    "PR start requires a fresh complete forge fetch; configure a provider (or an explicit test fixture stub)",
                ));
            }
            let latest = tx.sessions.get_all(Some(&context.id), false).await?;
            if !selection.accept_pr_comparison
                && latest.iter().any(|s| {
                    s.completed_at.as_ref().is_some_and(|time| {
                        fresh
                            .last_refreshed_at
                            .as_ref()
                            .is_none_or(|stamp| stamp <= time)
                    })
                })
            {
                return Err(invalid(
                    "PR start requires a new live refresh after the previous session",
                ));
            }
        }
        if comparison_kind != ComparisonKind::Notes
            && !tx
                .review_contexts
                .advance_revision(&context.id, guard.and_then(MutationGuard::write_version))
                .await?
        {
            return Err(Error::Conflict(
                "Review data changed before creating the attempt".into(),
            ));
        }
        let s = Session {
            id: uuid::Uuid::new_v4(),
            context_id: context.id,
            creator: actor.clone(),
            status: SessionStatus::Active,
            commits,
            comparison_kind,
            created_at: now(),
            forge_review_id: None,
            submission_status: if context.key.is_forge_backed() {
                "unsubmitted"
            } else {
                "local"
            }
            .into(),
            submission_error: None,
            draft_body: None,
        };
        tx.sessions.insert(&sessions::Model::from(&s)).await?;
        Self::record_mutation(&tx, &context.id, &actor.id, "start_session", guard, &s).await?;
        tx.commit().await?;
        Ok(s)
    }

    pub async fn complete(
        &self,
        context_id: &uuid::Uuid,
        actor: &Actor,
        input: Completion,
        request_key: &str,
        exact_session: Option<&uuid::Uuid>,
        guard: Option<&MutationGuard>,
    ) -> Result<Session> {
        let guard = guard
            .map(|g| g.bind(&(context_id, exact_session, &input, request_key)))
            .transpose()?;
        let guard = guard.as_ref();
        tracing::debug!("Review action: complete");
        let actor = &self.auth.validate_requester(actor).await?;
        if request_key.trim().is_empty() {
            return Err(invalid("Completion requires a nonempty retry key"));
        }
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_string(&(input.clone(), &actor.id))?)
        );
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<Session>(&tx, context_id, &actor.id, "complete", guard, true)
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if input.submit
            && tx
                .mutations
                .get_result(context_id, &actor.id, request_key)
                .await?
                .is_some_and(|receipt| receipt.method == "rejected_submission")
        {
            return Err(Error::Conflict(
                "Rejected submission cannot be completed again; choose a new submission attempt"
                    .into(),
            ));
        }
        let row = tx
            .sessions
            .get_completion_by_key(context_id, request_key)
            .await?;
        if let Some(row) = row {
            let s = Self::session_from_db_model(row, &tx).await?;
            if s.completion().map(|c| c.request_hash.as_str()) != Some(&hash) {
                return Err(Error::Conflict(
                    "Retry key was already used with different content or actor".into(),
                ));
            }
            if exact_session.is_some_and(|id| s.id != *id) {
                return Err(Error::Conflict(
                    "Retry key belongs to another review attempt".into(),
                ));
            }
            Self::record_mutation(&tx, context_id, &actor.id, "complete", guard, &s).await?;
            tx.commit().await?;
            return Ok(s);
        }
        let row = tx
            .sessions
            .get_active(context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Active session".into()))?;
        if exact_session.is_some_and(|id| row.id != *id) {
            return Err(Error::Conflict(
                "Selected review is no longer the active attempt".into(),
            ));
        }
        let mut s = Self::session_from_db_model(row, &tx).await?;
        let context = self.load_context(&tx, context_id).await?;
        if context.key.is_forge_backed() {
            if !input
                .outcome
                .as_deref()
                .is_some_and(|v| ["approve", "reject", "comment"].contains(&v))
            {
                return Err(invalid(
                    "PR completion requires --outcome approve|reject|comment",
                ));
            }
        } else if input.outcome.is_some() || input.submit {
            return Err(invalid(
                "PR outcomes/submission are invalid for notes and local branches",
            ));
        }
        let rows = tx.comments.get_session_draft_comments(&s.id).await?;
        for mut c in Self::comments_from_db_models(rows, &tx).await? {
            c.visibility = "published".into();
            c.updated_at = now();
            tx.comments
                .update_comment(&comments::Model::from(&c))
                .await?;
        }
        let outcome = input.outcome.unwrap_or_else(|| {
            if context.key.is_notes() {
                "saved"
            } else {
                "ready-to-address"
            }
            .into()
        });
        s.complete(actor.clone(), outcome, input.body, request_key.into(), hash)?;
        if input.submit {
            s.submission_status = "unsubmitted".into();
            s.submission_error = Some("Submission pending provider confirmation".into());
        }
        tx.sessions.complete(&sessions::Model::from(&s)).await?;
        Self::record_mutation(&tx, context_id, &actor.id, "complete", guard, &s).await?;
        tx.commit().await?;
        Ok(s)
    }

    pub async fn get_file_reviews(
        &self,
        review_id: &uuid::Uuid,
        actor: &Actor,
    ) -> Result<Vec<(String, String)>> {
        self.auth.validate_requester(actor).await?;
        let session = self.get_session(review_id).await?;
        let context = self.get_context_by_id(&session.context_id).await?;
        if context.key.is_forge_backed() {
            return Err(invalid("PR file marks belong to the forge"));
        }
        Ok(self
            .database
            .review_contexts
            .get_file_reviews(&context.id, &actor.id)
            .await?)
    }

    pub async fn set_file_reviewed(
        &self,
        review_id: &uuid::Uuid,
        actor: &Actor,
        path: &str,
        content_identity: Option<&str>,
    ) -> Result<()> {
        self.auth.validate_requester(actor).await?;
        Anchor::File { path: path.into() }.validate()?;
        if content_identity.is_some_and(|value| value.is_empty() || value.len() > 4096) {
            return Err(invalid(
                "Reviewed content requires a bounded nonempty identity",
            ));
        }
        let session = self.get_session(review_id).await?;
        let context = self.get_context_by_id(&session.context_id).await?;
        if context.key.is_forge_backed() || session.comparison_kind == ComparisonKind::Notes {
            return Err(invalid("Local reviewed-file marks require a local review"));
        }
        self.database
            .review_contexts
            .set_file_reviewed(&context.id, &actor.id, path, content_identity)
            .await?;
        Ok(())
    }

    /// Resolve the internal active writer only for a mutation. Ordinary notes have no visible review.
    pub async fn resolve_write_scope(&self, scope: &ReviewScope, actor: &Actor) -> Result<Session> {
        if let ReviewScope::Review { review_id } = scope {
            return self.get_session(review_id).await;
        }
        let ReviewScope::Notes { path } = scope else {
            unreachable!()
        };
        let path = Self::get_notes_path(path)?;
        let context = match self.get_scope_context(scope).await? {
            Some(context) => context,
            None => {
                self.ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Notes { path },
                    },
                    None,
                )
                .await?
            }
        };
        if let Some(active) = self.database.sessions.get_active(&context.id).await? {
            return self.get_session(&active.id).await;
        }
        self.start_session(
            &context,
            actor,
            CommitContext::default(),
            ComparisonKind::Notes,
            None,
        )
        .await
    }

    /// Freeze every outgoing comment and the review completion in one transaction.
    /// A crash exposes either the original active drafts or durable sending state,
    /// never half-staged comments without a session retry identity.
    pub async fn prepare_submission(
        &self,
        session_id: &uuid::Uuid,
        actor: &Actor,
        input: Completion,
        request_key: &str,
        prepared: &[(Comment, String, String)],
        expected_version: Option<i64>,
    ) -> Result<Session> {
        let actor = self.auth.validate_requester(actor).await?;
        if request_key.trim().is_empty()
            || !input.submit
            || !input
                .outcome
                .as_deref()
                .is_some_and(|v| ["approve", "reject", "comment"].contains(&v))
        {
            return Err(invalid(
                "Submission requires an outcome and nonempty retry key",
            ));
        }
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(input.clone(), actor.id))?)
        );
        let seed = self.get_session(session_id).await?;
        let context_id = &seed.context_id;
        let tx = self.database.begin().await?;
        if !tx
            .review_contexts
            .advance_revision(context_id, expected_version)
            .await?
        {
            return Err(Error::Conflict(
                "Review changed before submission; refresh and keep the final body".into(),
            ));
        }
        if tx.mutations.get_pending(context_id, None).await? {
            return Err(Error::Conflict(
                "Resolve pending remote comment changes before submitting".into(),
            ));
        }
        let context = self.load_context(&tx, context_id).await?;
        if !context.key.is_forge_backed() {
            return Err(invalid("Only forge-backed reviews can be submitted"));
        }
        if tx
            .mutations
            .get_result(context_id, &actor.id, request_key)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(
                "Submission retry key was already used; choose a new submission attempt".into(),
            ));
        }
        if tx
            .sessions
            .get_completion_by_key(context_id, request_key)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(
                "Submission was already prepared; reconcile before retrying".into(),
            ));
        }
        let mut session = self.require_active_session(&tx, session_id).await?;
        if session.creator.id != actor.id {
            return Err(invalid("Only the session creator can submit this review"));
        }
        if session.submission_status != "unsubmitted" {
            return Err(Error::Conflict(
                "Resolve pending draft synchronization before submitting".into(),
            ));
        }
        let comments =
            Self::comments_from_db_models(tx.comments.get_comments(context_id).await?, &tx).await?;
        let outgoing: Vec<_> = comments
            .iter()
            .filter(|c| c.origin_session_id == Some(session.id) && c.forge_comment_id.is_none())
            .collect();
        let ids: HashSet<_> = prepared.iter().map(|(c, _, _)| c.id).collect();
        if ids.len() != prepared.len()
            || outgoing.len() != prepared.len()
            || outgoing.iter().any(|c| !ids.contains(&c.id))
        {
            return Err(Error::Conflict(
                "Draft set changed while preparing submission; refresh and retry".into(),
            ));
        }
        for (expected, rendered, key) in prepared {
            let current = outgoing
                .iter()
                .find(|c| c.id == expected.id)
                .ok_or_else(|| Error::NotFound("Outgoing comment".into()))?;
            if current.body != expected.body
                || current.author != expected.author
                || current.updated_at != expected.updated_at
                || current.origin_session_id != Some(session.id)
                || current.posting.state() != db::types::PostingState::Unposted
                || key.trim().is_empty()
            {
                return Err(Error::Conflict(
                    "Draft content or posting state changed while preparing submission".into(),
                ));
            }
            let mut staged = (**current).clone();
            staged.visibility = "published".into();
            staged.posting = PostingStatus::Sending;
            staged.posting_request_key = Some(key.clone());
            staged.payload_hash = Some(format!("{:x}", Sha256::digest(rendered.as_bytes())));
            staged.forge_body = Some(rendered.clone());
            staged.updated_at = now();
            tx.comments
                .update_comment(&comments::Model::from(&staged))
                .await?;
        }
        for mut comment in comments {
            if comment.origin_session_id == Some(session.id)
                && comment.visibility == "draft"
                && !ids.contains(&comment.id)
            {
                comment.visibility = "published".into();
                comment.updated_at = now();
                tx.comments
                    .update_comment(&comments::Model::from(&comment))
                    .await?;
            }
        }
        session.complete(
            actor,
            input.outcome.unwrap(),
            input.body,
            request_key.into(),
            hash,
        )?;
        session.submission_status = "sending".into();
        session.submission_error = None;
        if !tx
            .sessions
            .complete(&sessions::Model::from(&session))
            .await?
        {
            return Err(Error::Conflict(
                "Active session changed during submission preparation".into(),
            ));
        }
        tx.commit().await?;
        Ok(session)
    }

    /// Reopen only after the forge confirms the final event was rejected and all drafts are accounted for.
    pub(crate) async fn resume_rejected_submission(
        &self,
        session_id: &Uuid,
        actor: &Actor,
    ) -> Result<crate::models::Session> {
        let actor = self.auth.validate_requester(actor).await?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let row = tx
            .sessions
            .get_by_id(session_id)
            .await?
            .ok_or_else(|| Error::NotFound("Review".into()))?;
        let mut session = Self::session_from_db_model(row.clone(), &tx).await?;
        if session.creator.id != actor.id
            || !self
                .load_context(&tx, &session.context_id)
                .await?
                .key
                .is_forge_backed()
        {
            return Err(invalid(
                "Only the PR review owner can recover its rejected submission",
            ));
        }
        if session.state() == "active" && session.submission_status == "unsubmitted" {
            tx.commit().await?;
            return self.get_session(session_id).await;
        }
        let completion = session
            .completion()
            .ok_or_else(|| invalid("No frozen submission to recover"))?
            .clone();
        if tx.sessions.get_active(&session.context_id).await?.is_some() {
            return Err(Error::Conflict(
                "Another review is active; the rejected review was preserved".into(),
            ));
        }
        let comments = Self::comments_from_db_models(
            tx.comments.get_comments(&session.context_id).await?,
            &tx,
        )
        .await?;
        for comment in comments
            .iter()
            .filter(|c| c.origin_session_id == Some(session.id))
        {
            if comment.forge_comment_id.is_none()
                || ["sending", "uncertain"].contains(&comment.posting.state().as_str())
            {
                return Err(Error::Conflict(
                    "Recover unconfirmed comment writes before reopening the rejected review"
                        .into(),
                ));
            }
        }
        session.resume_rejected_submission()?;
        tx.review_contexts
            .advance_revision(&session.context_id, None)
            .await?;
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        // A delayed retry of the rejected key must never publish a second outcome.
        tx.mutations
            .insert_result(&mutation_receipts::Model {
                context_id: session.context_id,
                actor_id: actor.id,
                request_key: completion.request_key,
                method: "rejected_submission".into(),
                request_hash: completion.request_hash,
                result_json: "{}".into(),
                version,
                object_id: Some(session.id),
                pending: false,
            })
            .await?;
        if !tx.sessions.reopen_rejected_submission(&row).await? {
            return Err(Error::Conflict(
                "Submission state changed during recovery".into(),
            ));
        }
        for mut comment in comments {
            if comment.origin_session_id == Some(session.id)
                && comment.forge_object_kind.as_deref() == Some("review_comment")
            {
                comment.visibility = "draft".into();
                tx.comments
                    .update_comment(&comments::Model::from(&comment))
                    .await?;
            }
        }
        tx.commit().await?;
        self.get_session(session_id).await
    }

    // Threads -----------------------------------------------------------------

    pub async fn resolve(
        &self,
        session_id: &uuid::Uuid,
        thread_id: &uuid::Uuid,
        actor: &Actor,
        resolved: bool,
        guard: Option<&MutationGuard>,
    ) -> Result<Thread> {
        let guard = guard
            .map(|g| g.bind_attempt(session_id, &(thread_id, resolved)))
            .transpose()?;
        let guard = guard.as_ref();
        tracing::debug!("Review action: resolve");
        let actor = &self.auth.validate_requester(actor).await?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<Thread>(&tx, &seed.context_id, &actor.id, "resolve", guard, true)
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if guard.is_some() {
            let context = self.load_context(&tx, &seed.context_id).await?;
            Self::validate_local_context(&context).await?;
        }
        self.require_active_session(&tx, session_id).await?;
        let mut t = self.load_thread(&tx, thread_id, &seed.context_id).await?;
        if let Some(guard) = guard {
            guard.validate_object(ObjectKind::Thread, t.id, &t)?;
        }
        if t.forge_thread_id.is_some() {
            return Err(invalid(
                "Remote thread resolution is deferred; no forge action was performed",
            ));
        }
        t.state = if resolved { "resolved" } else { "open" }.into();
        t.resolved_by = resolved.then(|| actor.clone());
        t.resolved_at = resolved.then(now);
        t.updated_by = Some(actor.clone());
        t.updated_at = now();
        tx.comments.update_thread(&threads::Model::from(&t)).await?;
        Self::record_mutation(&tx, &seed.context_id, &actor.id, "resolve", guard, &t).await?;
        tx.commit().await?;
        Ok(t)
    }

    // Comments and notes ------------------------------------------------------

    pub async fn get_comment(&self, context: &uuid::Uuid, id: &uuid::Uuid) -> Result<Comment> {
        let row = self
            .database
            .comments
            .get_comment_by_id(context, id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment in selected context".into()))?;
        Self::comment_from_db_model(row, &self.database).await
    }

    pub async fn add_comment(
        &self,
        session_id: &uuid::Uuid,
        actor: &Actor,
        body: &str,
        anchor: Anchor,
        evidence: Option<AnchorEvidence>,
        guard: Option<&MutationGuard>,
    ) -> Result<Comment> {
        let guard = guard
            .map(|g| g.bind_attempt(session_id, &(body, &anchor, &evidence)))
            .transpose()?;
        let guard = guard.as_ref();
        tracing::debug!("Review action: add_comment");
        let actor = &self.auth.validate_requester(actor).await?;
        Comment::validate_body(body)?;
        anchor.validate()?;
        let seed = self.get_session(session_id).await?;
        if let Some(evidence) = &evidence {
            let context = self.get_context_by_id(&seed.context_id).await?;
            if context.key.is_forge_backed() {
                if let Some(guard) = guard
                    && let Some(saved) = self
                        .get_saved_mutation(&seed.context_id, &actor.id, "add_comment", guard)
                        .await?
                {
                    return Ok(saved);
                }
                self.validate_forge_evidence(&context, &seed, &anchor, evidence)
                    .await?;
            }
        }
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<Comment>(&tx, &seed.context_id, &actor.id, "add_comment", guard, true)
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        let s = self.require_active_session(&tx, session_id).await?;
        let context = self.load_context(&tx, &s.context_id).await?;
        if guard.is_some() {
            Self::validate_local_context(&context).await?;
        }
        if let Some(evidence) = &evidence {
            self.validate_anchor_evidence(
                &context,
                &s,
                &anchor,
                evidence,
                guard.and_then(|g| g.scope.as_ref()),
            )
            .await?;
        } else if matches!(anchor, Anchor::Line { .. }) {
            return Err(invalid("Line comments require saved source evidence"));
        }
        let source_path = Self::get_saved_source_location(
            &context,
            &anchor,
            guard.and_then(|g| g.scope.as_ref()),
        )?
        .map(|path| path.to_string_lossy().into_owned());
        let t = Thread {
            evidence,
            current_path: None,
            source_path,
            orphan_reason: None,
            forge_observation: None,
            id: uuid::Uuid::new_v4(),
            context_id: s.context_id,
            origin_session_id: Some(s.id),
            author: actor.clone(),
            anchor,
            state: "open".into(),
            resolved_by: None,
            resolved_at: None,
            updated_by: Some(actor.clone()),
            updated_at: now(),
            forge_thread_id: None,
        };
        tx.comments.insert_thread(&threads::Model::from(&t)).await?;
        let c = Comment::local(&s, &t, actor, body, 0);
        tx.comments
            .insert_comment(&comments::Model::from(&c))
            .await?;
        Self::record_mutation(&tx, &seed.context_id, &actor.id, "add_comment", guard, &c).await?;
        tx.commit().await?;
        Ok(c)
    }

    /// Copy selected root notes atomically, retaining original authors and durable provenance.
    pub async fn promote_notes(
        &self,
        source_context: &uuid::Uuid,
        target_session: &uuid::Uuid,
        actor: &Actor,
        selection: NotePromotion,
    ) -> Result<Vec<Comment>> {
        let ids = &selection.comment_ids;
        let anchors = &selection.anchors;
        let evidence = &selection.evidence;
        let expected_source_version = selection.expected_source_version;
        let guard = selection.guard.bind(&(
            source_context,
            target_session,
            ids,
            anchors,
            evidence,
            expected_source_version,
        ))?;
        let guard = Some(&guard);
        self.auth.validate_requester(actor).await?;
        if ids.is_empty()
            || ids.len() > 500
            || ids.len() != anchors.len()
            || (!evidence.is_empty() && evidence.len() != ids.len())
        {
            return Err(invalid(
                "Promotion requires a retry key, selected notes and an explicit anchor per note",
            ));
        }
        let source = self.get_context_by_id(source_context).await?;
        if !source.key.is_notes() {
            return Err(invalid("Promotion source must be a notes context"));
        }
        let seed = self.get_session(target_session).await?;
        if seed.context_id == *source_context {
            return Err(invalid("Choose a distinct target review context"));
        }
        let target_context = self.get_context_by_id(&seed.context_id).await?;
        if target_context.key.is_forge_backed() {
            if let Some(guard) = guard
                && let Some(saved) = self
                    .get_saved_mutation(&seed.context_id, &actor.id, "promote_notes", guard)
                    .await?
            {
                return Ok(saved);
            }
            for (anchor, evidence) in anchors.iter().zip(evidence) {
                if let Some(evidence) = evidence {
                    self.validate_forge_evidence(&target_context, &seed, anchor, evidence)
                        .await?;
                }
            }
        }
        let tx = self.database.begin().await?;
        // Stable lock order prevents cross-context promotion deadlocks.
        let mut contexts = [*source_context, seed.context_id];
        contexts.sort();
        for context in contexts {
            if !tx.review_contexts.lock_for_write(&context).await? {
                return Err(Error::NotFound("Promotion context".into()));
            }
        }
        if let Some(saved) = self
            .begin_mutation::<Vec<Comment>>(
                &tx,
                &seed.context_id,
                &actor.id,
                "promote_notes",
                guard,
                true,
            )
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if !tx
            .review_contexts
            .advance_revision(source_context, expected_source_version)
            .await?
        {
            return Err(Error::Conflict(
                "Source notes changed; read them before promoting".into(),
            ));
        }
        let target = self.require_active_session(&tx, target_session).await?;
        let mut result = Vec::new();
        let target_context = self.load_context(&tx, &target.context_id).await?;
        if guard.is_some() {
            Self::validate_local_context(&target_context).await?;
        }
        for (index, (source_id, anchor)) in ids.iter().zip(anchors).enumerate() {
            let target_evidence = evidence.get(index).and_then(Option::as_ref);
            anchor.validate()?;
            let row = tx
                .comments
                .get_comment_by_id(source_context, source_id)
                .await?
                .ok_or_else(|| Error::NotFound("Selected source note".into()))?;
            let source = Self::comment_from_db_model(row, &tx).await?;
            if source.position != 0 {
                return Err(invalid(
                    "Select root notes; replies remain in their source thread",
                ));
            }
            if let Some(evidence) = target_evidence {
                self.validate_anchor_evidence(&target_context, &target, anchor, evidence, None)
                    .await?;
            } else if matches!(anchor, Anchor::Line { .. }) {
                return Err(invalid("Line promotion requires target source evidence"));
            }
            let source_path = Self::get_saved_source_location(&target_context, anchor, None)?
                .map(|path| path.to_string_lossy().into_owned());
            let thread = Thread {
                evidence: target_evidence.cloned(),
                current_path: None,
                source_path,
                orphan_reason: None,
                forge_observation: None,
                id: uuid::Uuid::new_v4(),
                context_id: target.context_id,
                origin_session_id: Some(target.id),
                author: source.author.clone(),
                anchor: anchor.clone(),
                state: "open".into(),
                resolved_by: None,
                resolved_at: None,
                updated_by: Some(actor.clone()),
                updated_at: now(),
                forge_thread_id: None,
            };
            let comment = Comment::promote(&source, &target, &thread, actor);
            tx.comments
                .insert_thread(&threads::Model::from(&thread))
                .await?;
            tx.comments
                .insert_comment(&comments::Model::from(&comment))
                .await?;
            result.push(comment);
        }
        Self::record_mutation(
            &tx,
            &seed.context_id,
            &actor.id,
            "promote_notes",
            guard,
            &result,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Edit an owned, unposted draft without changing its attribution or anchor.
    pub async fn edit_comment(
        &self,
        session_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        actor: &Actor,
        body: &str,
        guard: Option<&MutationGuard>,
    ) -> Result<Comment> {
        let guard = guard
            .map(|g| g.bind_attempt(session_id, &(comment_id, body)))
            .transpose()?;
        let guard = guard.as_ref();
        let actor = self.auth.validate_requester(actor).await?;
        Comment::validate_body(body)?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<Comment>(
                &tx,
                &seed.context_id,
                &actor.id,
                "edit_comment",
                guard,
                true,
            )
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if guard.is_some() {
            let context = self.load_context(&tx, &seed.context_id).await?;
            Self::validate_local_context(&context).await?;
        }
        self.require_active_session(&tx, session_id).await?;
        let row = tx
            .comments
            .get_comment_by_id(&seed.context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment".into()))?;
        let mut comment = Self::comment_from_db_model(row, &tx).await?;
        if let Some(guard) = guard {
            guard.validate_object(ObjectKind::Comment, comment.id, &comment)?;
        }
        if comment.owner_id != Some(actor.id)
            || comment.visibility != "draft"
            || comment.forge_comment_id.is_some()
            || comment.posting.state().as_str() != "unposted"
        {
            return Err(invalid("Only your unposted draft comments can be edited"));
        }
        comment.body = body.into();
        comment.updated_at = now();
        tx.comments
            .update_comment(&comments::Model::from(&comment))
            .await?;
        Self::record_mutation(
            &tx,
            &seed.context_id,
            &actor.id,
            "edit_comment",
            guard,
            &comment,
        )
        .await?;
        tx.commit().await?;
        Ok(comment)
    }

    /// Delete an owned unposted draft; root deletion requires an empty reply list.
    pub async fn delete_comment(
        &self,
        session_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        actor: &Actor,
        guard: Option<&MutationGuard>,
    ) -> Result<()> {
        let guard = guard
            .map(|g| g.bind_attempt(session_id, comment_id))
            .transpose()?;
        let guard = guard.as_ref();
        let actor = self.auth.validate_requester(actor).await?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<()>(
                &tx,
                &seed.context_id,
                &actor.id,
                "delete_comment",
                guard,
                true,
            )
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if guard.is_some() {
            let context = self.load_context(&tx, &seed.context_id).await?;
            Self::validate_local_context(&context).await?;
        }
        self.require_active_session(&tx, session_id).await?;
        let row = tx
            .comments
            .get_comment_by_id(&seed.context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment".into()))?;
        if let Some(guard) = guard {
            let snapshot = Self::comment_from_db_model(row.clone(), &tx).await?;
            guard.validate_object(ObjectKind::Comment, snapshot.id, &snapshot)?;
        }
        if row.owner_id != Some(actor.id)
            || row.visibility != "draft"
            || row.forge_comment_id.is_some()
            || row.posting_state != db::types::PostingState::Unposted
        {
            return Err(invalid("Only your unposted draft comments can be deleted"));
        }
        if row.position == 0
            && tx
                .comments
                .get_comments(&seed.context_id)
                .await?
                .iter()
                .any(|c| c.thread_id == row.thread_id && c.id != row.id)
        {
            return Err(Error::Conflict(
                "Delete replies before deleting their root".into(),
            ));
        }
        tx.comments
            .delete_draft_comment(&seed.context_id, comment_id)
            .await?;
        if row.position == 0 {
            tx.comments
                .delete_empty_thread(&seed.context_id, &row.thread_id)
                .await?;
        }
        Self::record_mutation(
            &tx,
            &seed.context_id,
            &actor.id,
            "delete_comment",
            guard,
            &(),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn reply(
        &self,
        session_id: &uuid::Uuid,
        thread_id: &uuid::Uuid,
        actor: &Actor,
        body: &str,
        guard: Option<&MutationGuard>,
    ) -> Result<Comment> {
        let guard = guard
            .map(|g| g.bind_attempt(session_id, &(thread_id, body)))
            .transpose()?;
        let guard = guard.as_ref();
        tracing::debug!("Review action: reply");
        let actor = &self.auth.validate_requester(actor).await?;
        Comment::validate_body(body)?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<Comment>(&tx, &seed.context_id, &actor.id, "reply", guard, true)
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if guard.is_some() {
            let context = self.load_context(&tx, &seed.context_id).await?;
            Self::validate_local_context(&context).await?;
        }
        let s = self.require_active_session(&tx, session_id).await?;
        let t = self.load_thread(&tx, thread_id, &s.context_id).await?;
        if let Some(guard) = guard {
            guard.validate_object(ObjectKind::Thread, t.id, &t)?;
        }
        let position = tx.comments.get_next_reply_position(thread_id).await?;
        let c = Comment::local(&s, &t, actor, body, position);
        tx.comments
            .insert_comment(&comments::Model::from(&c))
            .await?;
        Self::record_mutation(&tx, &seed.context_id, &actor.id, "reply", guard, &c).await?;
        tx.commit().await?;
        Ok(c)
    }

    // Conversations and export ------------------------------------------------

    /// Page comments in stable thread/reply order. Reads never create notes or load file bodies.
    pub async fn get_conversation_page(
        &self,
        scope: &ReviewScope,
        paths: Option<&[String]>,
        include_drafts: bool,
        page_token: &str,
        page_size: u32,
    ) -> Result<(Option<Conversation>, String)> {
        let Some(context) = self.get_scope_context(scope).await? else {
            if !page_token.is_empty() {
                return Err(invalid("Page token has no matching notes scope"));
            }
            return Ok((None, String::new()));
        };
        if paths.is_some_and(|p| p.len() > 500) {
            return Err(invalid("Visible-file filter exceeds 500 paths"));
        }
        if let Some(paths) = paths {
            for path in paths {
                Anchor::File { path: path.clone() }.validate()?;
            }
        }
        // File reads use the selected notes scope and confirmed path associations, never neighboring feedback.
        let notes_paths = if let ReviewScope::Notes { path } = scope {
            let ContextKey::Local {
                location: LocalLocation::Notes { path: original },
            } = &context.key
            else {
                return Err(invalid("Expected notes context"));
            };
            let selected = Self::get_notes_path(path)?;
            let current = context.current_notes_path.as_ref().unwrap_or(original);
            if std::path::Path::new(&selected).is_dir()
                || (selected == *original && context.current_notes_path.is_none())
            {
                None
            } else {
                let mut aliases = vec![selected.clone()];
                for location in [original, current] {
                    let location = std::path::Path::new(location);
                    let root = if location.is_dir()
                        || std::path::Path::new(&selected).starts_with(location)
                    {
                        location
                    } else {
                        location
                            .parent()
                            .ok_or_else(|| invalid("Notes file has no parent"))?
                    };
                    if let Ok(relative) = std::path::Path::new(&selected).strip_prefix(root) {
                        aliases.push(relative.to_string_lossy().into_owned());
                    }
                }
                Some(aliases)
            }
        } else if context.key.is_notes() {
            if let Some(paths) = paths {
                let root = context
                    .current_notes_path
                    .as_ref()
                    .or(match &context.key {
                        ContextKey::Local {
                            location: LocalLocation::Notes { path },
                        } => Some(path),
                        _ => None,
                    })
                    .ok_or_else(|| invalid("Notes have no source root"))?;
                let mut aliases = paths.to_vec();
                aliases.extend(paths.iter().map(|path| {
                    std::path::Path::new(root)
                        .join(path)
                        .to_string_lossy()
                        .into_owned()
                }));
                Some(aliases)
            } else {
                None
            }
        } else {
            None
        };
        let paths = notes_paths.as_deref().or(paths);
        let filter = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(paths, include_drafts))?)
        );
        let prefix = format!("{}:{}:{filter}:", context.id, context.revision);
        let after = if page_token.is_empty() {
            None
        } else {
            let cursor = page_token.strip_prefix(&prefix).ok_or_else(|| {
                Error::Conflict("Conversation or query changed; restart pagination".into())
            })?;
            let (thread, position) = cursor
                .split_once(':')
                .ok_or_else(|| invalid("Invalid conversation cursor"))?;
            Some((
                uuid::Uuid::parse_str(thread)
                    .map_err(|_| invalid("Invalid conversation cursor"))?,
                position
                    .parse::<i64>()
                    .map_err(|_| invalid("Invalid conversation cursor"))?,
            ))
        };
        let size = if page_size == 0 {
            100
        } else {
            page_size.min(500)
        } as usize;
        let mut rows = self
            .database
            .comments
            .get_comment_page(&context.id, paths, include_drafts, after, size as u64 + 1)
            .await?;
        let more = rows.len() > size;
        rows.truncate(size);
        let next = if more {
            rows.last()
                .map(|c| format!("{prefix}{}:{}", c.thread_id, c.position))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let ids: Vec<_> = rows
            .iter()
            .map(|c| c.thread_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let comments = Self::comments_from_db_models(rows, &self.database).await?;
        let threads = Self::threads_from_db_models(
            self.database
                .comments
                .get_threads_by_ids(&context.id, &ids)
                .await?,
            &self.database,
        )
        .await?;
        if self.get_context_by_id(&context.id).await?.revision != context.revision {
            return Err(Error::Conflict(
                "Conversation changed during the read; retry".into(),
            ));
        }
        Ok((
            Some(Conversation {
                context,
                sessions: vec![],
                threads,
                comments,
            }),
            next,
        ))
    }

    /// Read all feedback for export, using stable pages and preserving immutable original anchors.
    pub async fn get_scope_conversation(
        &self,
        scope: &ReviewScope,
    ) -> Result<Option<Conversation>> {
        let (mut combined, mut token) = self
            .get_conversation_page(scope, None, true, "", 500)
            .await?;
        while !token.is_empty() {
            let (page, next) = self
                .get_conversation_page(scope, None, true, &token, 500)
                .await?;
            if let (Some(combined), Some(page)) = (&mut combined, page) {
                for thread in page.threads {
                    if !combined.threads.iter().any(|t| t.id == thread.id) {
                        combined.threads.push(thread);
                    }
                }
                combined.comments.extend(page.comments);
            }
            token = next;
        }
        Ok(combined)
    }

    pub async fn get_conversation(
        &self,
        context: &uuid::Uuid,
        include_drafts: bool,
    ) -> Result<Conversation> {
        let comments: Vec<Comment> = Self::comments_from_db_models(
            self.database.comments.get_comments(context).await?,
            &self.database,
        )
        .await?
        .into_iter()
        .filter(|c| include_drafts || c.visibility == "published")
        .collect();
        let visible: std::collections::HashSet<_> = comments.iter().map(|c| c.thread_id).collect();
        let threads: Vec<Thread> = Self::threads_from_db_models(
            self.database.comments.get_threads(context).await?,
            &self.database,
        )
        .await?
        .into_iter()
        .filter(|t| visible.contains(&t.id))
        .collect();
        let sessions = Self::sessions_from_db_models(
            self.database.sessions.get_all(Some(context), false).await?,
            &self.database,
        )
        .await?;
        let current_context = self.get_context_by_id(context).await?;
        Ok(Conversation {
            context: current_context,
            sessions,
            threads,
            comments,
        })
    }

    pub async fn get_export(
        &self,
        scope: &ReviewScope,
        selected_roots: &[uuid::Uuid],
    ) -> Result<String> {
        let Some(mut conversation) = self.get_scope_conversation(scope).await? else {
            return Ok("# Review feedback\n\nNo comments yet.\n".into());
        };
        let session = match scope {
            ReviewScope::Review { review_id } => self.get_session(review_id).await?,
            ReviewScope::Notes { .. } => {
                self.get_selected_session(&conversation.context.id, None, false)
                    .await?
            }
        };
        if !selected_roots.is_empty() {
            let mut ids = HashSet::new();
            for id in selected_roots {
                let root = conversation
                    .comments
                    .iter()
                    .find(|c| {
                        c.id == *id
                            && !conversation.comments.iter().any(|earlier| {
                                earlier.thread_id == c.thread_id && earlier.position < c.position
                            })
                    })
                    .ok_or_else(|| Error::NotFound("Selected root feedback in scope".into()))?;
                ids.insert(root.thread_id);
            }
            conversation.threads.retain(|t| ids.contains(&t.id));
            conversation.comments.retain(|c| ids.contains(&c.thread_id));
        }
        conversation.sessions = Self::sessions_from_db_models(
            self.database
                .sessions
                .get_all(Some(&conversation.context.id), false)
                .await?,
            &self.database,
        )
        .await?;
        if self
            .get_context_by_id(&conversation.context.id)
            .await?
            .revision
            != conversation.context.revision
        {
            return Err(Error::Conflict(
                "Review changed during export; retry".into(),
            ));
        }
        Ok(conversation.export_feedback(&session))
    }

    // Anchors and locations ---------------------------------------------------

    /// Apply only validated source observations. Hidden files and failed reads never imply deletion.
    pub async fn refresh_locations(
        &self,
        scope: &ReviewScope,
        actor: &Actor,
        changes: &[SourceChange],
        guard: &MutationGuard,
    ) -> Result<i64> {
        use crate::database::types::SourceKind;
        if changes.len() > 500 {
            return Err(invalid("Refresh exceeds 500 source observations"));
        }
        let actor = self.auth.validate_requester(actor).await?;
        let Some(context) = self.get_scope_context(scope).await? else {
            if !changes.is_empty() {
                return Err(Error::NotFound("Notes with saved discussions".into()));
            }
            return Ok(0);
        };
        if context.key.is_forge_backed() && !changes.is_empty() {
            return Err(invalid("Forge locations come from provider observations"));
        }
        let guard = guard.bind(&(scope, changes))?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<i64>(
                &tx,
                &context.id,
                &actor.id,
                "refresh_locations",
                Some(&guard),
                true,
            )
            .await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        if !changes.is_empty() {
            Self::validate_local_context(&context).await?;
        }
        if changes
            .iter()
            .any(|c| matches!(c, SourceChange::LocationMoved { .. }))
        {
            if changes.len() != 1 {
                return Err(invalid("A scope move must be one atomic observation"));
            }
            if let SourceChange::LocationMoved {
                previous_path,
                current_path,
            } = &changes[0]
            {
                self.apply_location_move(&tx, &context, scope, previous_path, current_path)
                    .await?;
            }
        }
        let root = match &context.key {
            ContextKey::Local {
                location: LocalLocation::Notes { path },
            } => {
                let path =
                    std::path::PathBuf::from(context.current_notes_path.as_ref().unwrap_or(path));
                let selected_is_child = matches!(scope, ReviewScope::Notes { path: selected } if std::path::Path::new(selected).starts_with(&path) && std::path::Path::new(selected) != path);
                if matches!(scope, ReviewScope::Notes { .. })
                    && !path.is_dir()
                    && !selected_is_child
                {
                    path.parent()
                        .ok_or_else(|| invalid("Notes file has no parent"))?
                        .to_path_buf()
                } else {
                    path
                }
            }
            ContextKey::Local {
                location: LocalLocation::Branch { .. },
            } => context
                .paths
                .first()
                .map(std::path::PathBuf::from)
                .ok_or_else(|| invalid("Review has no working tree location"))?,
            ContextKey::ForgeBacked { .. } => std::path::PathBuf::new(),
        };
        for change in changes {
            let (id, previous, current, observed) = match change {
                SourceChange::LocationMoved { .. } => continue,
                SourceChange::Moved {
                    thread_id,
                    previous_path,
                    current_path,
                    source,
                } => (
                    thread_id,
                    Some(previous_path.as_str()),
                    current_path.as_str(),
                    Some(source),
                ),
                SourceChange::Deleted { thread_id, path } => {
                    (thread_id, Some(path.as_str()), path.as_str(), None)
                }
                SourceChange::WorkingSourceChanged {
                    thread_id,
                    path,
                    source,
                }
                | SourceChange::Restored {
                    thread_id,
                    path,
                    source,
                } => (thread_id, None, path.as_str(), Some(source)),
            };
            let relative_current = context.key.is_notes()
                && matches!(scope, ReviewScope::Review { .. })
                && !std::path::Path::new(current).is_absolute();
            let supplied_current = current;
            let saved_path = |path: &str| -> Result<String> {
                if context.key.is_notes() {
                    let absolute = if std::path::Path::new(path).is_absolute() {
                        std::path::PathBuf::from(path)
                    } else {
                        root.join(path)
                    };
                    Self::get_notes_path(&absolute.to_string_lossy())
                } else {
                    Anchor::File { path: path.into() }.validate()?;
                    Ok(path.into())
                }
            };
            let current = saved_path(current)?;
            let previous = previous.map(saved_path).transpose()?;
            let mut thread = self.load_thread(&tx, id, &context.id).await?;
            let original_path = match &thread.anchor {
                Anchor::File { path } | Anchor::Line { path, .. } => path,
                Anchor::General => return Err(invalid("General discussions have no file source")),
            };
            let known_path = saved_path(
                thread
                    .current_path
                    .as_deref()
                    .or(thread.source_path.as_deref())
                    .unwrap_or(original_path),
            )?;
            if previous.as_deref().is_some_and(|path| path != known_path) {
                return Err(Error::Conflict(
                    "Source observation does not match the current saved path".into(),
                ));
            }
            if observed.is_none() {
                let path = root.join(&current);
                let missing = match std::fs::symlink_metadata(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                    Err(e) => return Err(e.into()),
                    Ok(_) => false,
                };
                if !missing {
                    return Err(Error::Conflict("File deletion was not confirmed".into()));
                }
                thread.orphan_reason = Some(OrphanReason::FileDeleted);
            } else if let Some(observed) = observed {
                observed.validate()?;
                let (kind, commit) = match observed {
                    AnchorSource::Commit { sha, .. } => (SourceKind::Commit, Some(sha.clone())),
                    AnchorSource::IndexFile { .. } => (SourceKind::IndexFile, None),
                    AnchorSource::WorkingFile { .. } => (SourceKind::WorkingFile, None),
                    AnchorSource::LocalFile { .. } => (SourceKind::LocalFile, None),
                };
                if context.key.is_notes() != (kind == SourceKind::LocalFile) {
                    return Err(invalid("Observed source does not belong to this scope"));
                }
                let (source_root, path) = if context.key.is_notes() {
                    let absolute = std::path::Path::new(&current);
                    (
                        absolute
                            .parent()
                            .ok_or_else(|| invalid("Notes file has no parent"))?
                            .to_path_buf(),
                        absolute
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or_else(|| invalid("Notes file has no name"))?
                            .to_owned(),
                    )
                } else {
                    (root.clone(), current.clone())
                };
                let (actual, bytes) = tokio::task::spawn_blocking(move || {
                    super::workspace::get_source(&source_root, &path, kind, commit.as_deref())
                })
                .await
                .map_err(|_| invalid("Source observation failed"))??;
                if actual != *observed {
                    return Err(Error::Conflict(
                        "Observed source changed before refresh".into(),
                    ));
                }
                if matches!(change, SourceChange::WorkingSourceChanged { .. }) {
                    if !matches!(
                        thread.evidence.as_ref().map(|e| &e.source),
                        Some(AnchorSource::WorkingFile { .. })
                    ) {
                        return Err(invalid(
                            "Only unstaged working anchors use working-source orphaning",
                        ));
                    }
                    if thread
                        .evidence
                        .as_ref()
                        .is_some_and(|e| e.source.content_hash() == observed.content_hash())
                    {
                        return Err(invalid("Working source has not changed"));
                    }
                    thread.orphan_reason = Some(OrphanReason::WorkingSourceChanged);
                } else {
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|_| invalid("Source observation requires text"))?;
                    let supported = if matches!(change, SourceChange::Moved { .. }) {
                        // A reported move preserves file identity, even when its selected lines were lost.
                        // The request names the known old path and actual new source; absence is checked below.
                        true
                    } else if let Some(evidence) = &thread.evidence {
                        evidence.source.content_hash() == observed.content_hash()
                            || (!matches!(evidence.source, AnchorSource::WorkingFile { .. })
                                && evidence.get_matching_range(text).is_some())
                    } else {
                        false
                    };
                    if !supported {
                        return Err(Error::Conflict(
                            "Preserved evidence cannot confirm this move or restoration".into(),
                        ));
                    }
                    if let SourceChange::Moved { previous_path, .. } = change {
                        match std::fs::symlink_metadata(root.join(saved_path(previous_path)?)) {
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e.into()),
                            Ok(_) => {
                                return Err(Error::Conflict(
                                    "Move source still exists; a copy is not a confirmed move"
                                        .into(),
                                ));
                            }
                        }
                    }
                    thread.current_path = Some(if relative_current {
                        supplied_current.into()
                    } else {
                        current.clone()
                    });
                    thread.orphan_reason = None;
                    if matches!(scope, ReviewScope::Notes { .. }) {
                        tx.workspace
                            .insert_path(&workspace_paths::Model {
                                context_id: context.id,
                                path: root.join(&current).to_string_lossy().into_owned(),
                            })
                            .await?;
                    }
                }
            }
            tx.comments
                .update_thread(&threads::Model::from(&thread))
                .await?;
        }
        if !changes.is_empty() {
            Self::validate_local_context(&context).await?;
        }
        let version = tx
            .review_contexts
            .get_by_id(&context.id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        Self::record_mutation(
            &tx,
            &context.id,
            &actor.id,
            "refresh_locations",
            Some(&guard),
            &version,
        )
        .await?;
        tx.commit().await?;
        Ok(version)
    }

    // Forge synchronization and recovery --------------------------------------

    pub async fn record_refresh_failure(&self, context: &Context, message: &str) -> Result<()> {
        tracing::debug!("Review action: record_refresh_failure");
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &context.id).await?;
        let mut current = self.load_context(&tx, &context.id).await?;
        if !current.key.is_forge_backed() {
            return Err(invalid("Forge refresh requires a forge-backed context"));
        }
        current.refresh_status = "failed".into();
        current.refresh_error = Some(message.into());
        tx.review_contexts
            .update_refresh(&review_contexts::Model::from(&current))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn apply_remote(
        &self,
        context: &Context,
        mut snapshot: ForgeSnapshot,
        stub: bool,
    ) -> Result<Context> {
        tracing::debug!("Review action: apply_remote");
        let stored = self.get_context_by_id(&context.id).await?;
        if stored.key != context.key {
            return Err(invalid("Context key does not match persisted context"));
        }
        let repository_url = stored
            .key
            .repository_url()
            .ok_or_else(|| invalid("Forge refresh requires a forge-backed context"))?;

        if !snapshot.complete || snapshot.head.is_empty() {
            return Err(invalid(
                "Incomplete forge fetch; all pages and reviewed head are required",
            ));
        }
        let tx = self.database.begin().await?;
        let mut deleted = HashSet::new();
        for receipt in tx.mutations.get_remote_delete_receipts(&context.id).await? {
            let saved: ChangeReceipt = serde_json::from_str(&receipt.result_json)?;
            if let Some(key) = saved.deleted_remote {
                deleted.insert(key);
            }
        }
        for thread in &mut snapshot.threads {
            let previous_root = thread.comments.first().map(|comment| comment.id.clone());
            thread
                .comments
                .retain(|comment| !deleted.contains(&(comment.kind.clone(), comment.id.clone())));
            if let Some(root) = thread.comments.first().map(|comment| comment.id.clone())
                && previous_root.as_ref() != Some(&root)
            {
                for (index, comment) in thread.comments.iter_mut().enumerate() {
                    if comment.reply_to == previous_root {
                        comment.reply_to = (index > 0).then(|| root.clone());
                    }
                }
            }
        }
        snapshot
            .threads
            .retain(|thread| !thread.comments.is_empty());
        let mut seen = HashSet::new();
        let mut thread_ids = HashSet::new();
        let mut authors = vec![];
        for thread in &snapshot.threads {
            thread.anchor.validate()?;
            if thread.id.is_empty()
                || !thread_ids.insert(thread.id.clone())
                || thread.comments.is_empty()
            {
                return Err(invalid(
                    "Remote thread IDs must be unique, with a root comment",
                ));
            }
            let root = &thread.comments[0];
            for (i, c) in thread.comments.iter().enumerate() {
                if c.id.is_empty()
                    || c.author.account_id.is_empty()
                    || !["review_comment", "conversation_comment", "review"]
                        .contains(&c.kind.as_str())
                    || !seen.insert((c.kind.clone(), c.id.clone()))
                    || (i == 0 && c.reply_to.is_some())
                    || (i > 0 && c.reply_to.as_deref() != Some(&root.id))
                {
                    return Err(invalid(
                        "Invalid/incomplete remote root/reply graph or duplicate scoped ID",
                    ));
                }
                let actor = AuthService::find_or_create(
                    &tx,
                    &c.author.name,
                    ActorKind::Human {
                        email: c.author.email.clone(),
                    },
                    &format!(
                        "forge:{}",
                        serde_json::to_string(&(repository_url.origin(), &c.author.account_id))?
                    ),
                )
                .await?;
                authors.push(actor);
            }
        }
        let changed = tx
            .review_contexts
            .advance_revision(&context.id, Some(context.revision))
            .await?;
        if !changed {
            return Err(Error::Conflict(
                "Context changed while fetching; refresh again without overwriting local writes"
                    .into(),
            ));
        }
        let mut author_index = 0;
        for remote_thread in &snapshot.threads {
            let row = tx
                .comments
                .get_thread_by_remote_id(&context.id, &remote_thread.id)
                .await?;
            let mut thread = if let Some(row) = row {
                Self::thread_from_db_model(row, &tx).await?
            } else {
                let root = &remote_thread.comments[0];
                let existing = tx
                    .comments
                    .get_comment_by_remote_id(&context.id, &root.kind, &root.id)
                    .await?;
                if let Some(row) = existing {
                    let c = Self::comment_from_db_model(row, &tx).await?;
                    self.load_thread(&tx, &c.thread_id, &context.id).await?
                } else {
                    let t = Thread {
                        evidence: None,
                        current_path: None,
                        source_path: None,
                        orphan_reason: None,
                        forge_observation: None,
                        id: uuid::Uuid::new_v4(),
                        context_id: context.id,
                        origin_session_id: None,
                        author: authors[author_index].clone(),
                        anchor: remote_thread.anchor.clone(),
                        state: "open".into(),
                        resolved_by: None,
                        resolved_at: None,
                        updated_by: None,
                        updated_at: now(),
                        forge_thread_id: Some(remote_thread.id.clone()),
                    };
                    tx.comments.insert_thread(&threads::Model::from(&t)).await?;
                    t
                }
            };
            thread.forge_thread_id = Some(remote_thread.id.clone());
            thread.state = if remote_thread.resolved {
                "resolved"
            } else {
                "open"
            }
            .into();
            thread.resolved_by = None;
            thread.resolved_at = None;
            thread.updated_by = None;
            thread.updated_at = now();
            thread.forge_observation = remote_thread.observation.clone();
            thread.orphan_reason = remote_thread.orphan_reason;
            if let Some(observation) = &thread.forge_observation {
                thread.current_path = match &observation.current {
                    Some(Anchor::File { path } | Anchor::Line { path, .. }) => Some(path.clone()),
                    _ => thread.current_path,
                };
            }
            tx.comments
                .update_thread(&threads::Model::from(&thread))
                .await?;
            for remote in &remote_thread.comments {
                let actor = &authors[author_index];
                author_index += 1;
                let row = tx
                    .comments
                    .get_comment_by_remote_id(&context.id, &remote.kind, &remote.id)
                    .await?;
                let existing = row.is_some();
                let mut comment = if let Some(row) = row {
                    let c = Self::comment_from_db_model(row, &tx).await?;
                    if c.thread_id != thread.id {
                        return Err(Error::Conflict(
                            "Remote comment unexpectedly moved between threads".into(),
                        ));
                    }
                    if ["sending", "uncertain"].contains(&c.posting.state().as_str()) {
                        return Err(Error::Conflict(
                            "Pending remote write requires explicit reconciliation".into(),
                        ));
                    }
                    c
                } else {
                    let position = tx.comments.get_next_reply_position(&thread.id).await?;
                    Comment {
                        id: uuid::Uuid::new_v4(),
                        context_id: context.id,
                        thread_id: thread.id,
                        origin_session_id: None,
                        owner_id: None,
                        source_note_id: None,
                        author: actor.clone(),
                        body: remote.body.clone(),
                        position,
                        visibility: "published".into(),
                        created_at: now(),
                        updated_at: now(),
                        forge_comment_id: Some(remote.id.clone()),
                        forge_object_kind: Some(remote.kind.clone()),
                        forge_review_id: remote.review_id.clone(),
                        forge_body: None,
                        forge_updated_at: None,
                        posting_account_id: Some(remote.author.account_id.clone()),
                        posting: PostingStatus::Confirmed,
                        posting_request_key: None,
                        payload_hash: None,
                    }
                };
                // Preserve logical author. Strip only the exact payload previously rendered by us.
                if !existing || comment.forge_body.as_deref() != Some(&remote.body) {
                    comment.body = remote.body.clone();
                }
                comment.visibility = if remote.pending { "draft" } else { "published" }.into();
                comment.forge_body = Some(remote.body.clone());
                comment.forge_updated_at = Some(remote.updated_at.clone());
                comment.forge_review_id = remote.review_id.clone();

                comment.updated_at = now();
                if existing {
                    tx.comments
                        .update_comment(&comments::Model::from(&comment))
                        .await?;
                } else {
                    tx.comments
                        .insert_comment(&comments::Model::from(&comment))
                        .await?;
                }
            }
        }
        let rows = tx.comments.get_remote_comments(&context.id).await?;
        for c in Self::comments_from_db_models(rows, &tx).await? {
            if !seen.contains(&(
                c.forge_object_kind.clone().unwrap_or_default(),
                c.forge_comment_id.clone().unwrap_or_default(),
            )) {
                if tx.mutations.get_pending(&context.id, Some(&c.id)).await? {
                    continue;
                }
                tx.comments
                    .delete_remote_comment(&context.id, &c.id)
                    .await?;
                tx.comments
                    .delete_empty_thread(&context.id, &c.thread_id)
                    .await?;
            }
        }
        let mut current = context.clone();
        current.last_refreshed_at = Some(now());
        current.refreshed_head = Some(snapshot.head);
        current.refreshed_base = snapshot.base;
        current.forge_state = snapshot.state;
        current.refresh_status = if stub { "stub" } else { "live" }.into();
        current.refresh_error = None;
        tx.review_contexts
            .update_refresh(&review_contexts::Model::from(&current))
            .await?;
        tx.commit().await?;
        self.get_context_by_id(&context.id).await
    }

    pub async fn stage_post(
        &self,
        session_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        request_key: &str,
        expected: &Comment,
        rendered: &str,
    ) -> Result<String> {
        tracing::debug!("Review action: stage_post");
        if request_key.trim().is_empty() {
            return Err(invalid("Posting requires a retry key"));
        }
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let session = self.require_active_session(&tx, session_id).await?;
        let row = tx
            .comments
            .get_comment_by_id(&session.context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment in selected context".into()))?;
        let mut c = Self::comment_from_db_model(row, &tx).await?;
        if c.origin_session_id.as_ref() != Some(session_id) || c.forge_comment_id.is_some() {
            return Err(invalid(
                "Only this review's unposted comments may be submitted",
            ));
        }
        if ["sending", "uncertain", "confirmed"].contains(&c.posting.state().as_str()) {
            return Err(Error::Conflict(
                "Comment may already have been sent; reconcile before retrying".into(),
            ));
        }
        let context = self.load_context(&tx, &session.context_id).await?;
        if !context.key.is_forge_backed() {
            return Err(invalid("Only PR comments can be posted"));
        }
        if c.id != expected.id
            || c.body != expected.body
            || c.updated_at != expected.updated_at
            || c.author != expected.author
        {
            return Err(Error::Conflict(
                "Comment or identity changed while preparing payload; prepare again".into(),
            ));
        }
        let rendered = rendered.to_owned();
        c.posting = PostingStatus::Sending;
        c.posting_request_key = Some(request_key.into());
        c.payload_hash = Some(format!("{:x}", Sha256::digest(&rendered)));
        c.forge_body = Some(rendered.clone());
        tx.comments
            .update_comment(&comments::Model::from(&c))
            .await?;
        tx.commit().await?;
        Ok(rendered)
    }

    /// A definite provider rejection did not accept the payload; the draft is editable again.
    pub async fn reject_post(
        &self,
        context_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
    ) -> Result<()> {
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        let row = tx
            .comments
            .get_comment_by_id(context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment".into()))?;
        let mut comment = Self::comment_from_db_model(row, &tx).await?;
        if comment.forge_comment_id.is_some() || comment.posting.state().as_str() != "sending" {
            return Err(Error::Conflict(
                "Only a rejected sending draft can be reset".into(),
            ));
        }
        comment.posting = PostingStatus::Unposted;
        tx.comments
            .update_comment(&comments::Model::from(&comment))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn posting_uncertain(
        &self,
        context_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        message: &str,
    ) -> Result<()> {
        tracing::debug!("Review action: posting_uncertain");
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        if !self
            .load_context(&tx, context_id)
            .await?
            .key
            .is_forge_backed()
        {
            return Err(invalid("Remote posting requires a forge-backed context"));
        }
        let row = tx
            .comments
            .get_comment_by_id(context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("comments".into()))?;
        let mut c = Self::comment_from_db_model(row, &tx).await?;
        c.posting = PostingStatus::Uncertain {
            error: message.into(),
        };
        tx.comments
            .update_comment(&comments::Model::from(&c))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Supply only a verified provider receipt, never a body/footer guess.
    pub async fn confirm_post(
        &self,
        context_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        request_key: &str,
        receipt: PostReceipt,
    ) -> Result<()> {
        tracing::debug!("Review action: confirm_post");
        if receipt.comment_id.is_empty()
            || !["review_comment", "conversation_comment", "review"]
                .contains(&receipt.kind.as_str())
        {
            return Err(invalid("Invalid forge receipt"));
        }
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        if !self
            .load_context(&tx, context_id)
            .await?
            .key
            .is_forge_backed()
        {
            return Err(invalid("Remote posting requires a forge-backed context"));
        }
        let row = tx
            .comments
            .get_comment_by_id(context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("comments".into()))?;
        let mut c = Self::comment_from_db_model(row, &tx).await?;
        if c.posting_request_key.as_deref() != Some(request_key) {
            return Err(Error::Conflict(
                "Receipt request key does not match comment".into(),
            ));
        }
        if let Some(existing) = &c.forge_comment_id
            && existing != &receipt.comment_id
        {
            return Err(Error::Conflict(
                "Comment already linked to another remote ID".into(),
            ));
        }
        c.forge_comment_id = Some(receipt.comment_id);
        c.forge_object_kind = Some(receipt.kind);
        c.forge_review_id = receipt.review_id;
        c.posting_account_id = Some(receipt.posting_account_id);
        c.posting = PostingStatus::Confirmed;

        tx.comments
            .update_comment(&comments::Model::from(&c))
            .await?;
        if let Some(remote_thread) = receipt.thread_id {
            let mut t = self.load_thread(&tx, &c.thread_id, context_id).await?;
            if t.forge_thread_id
                .as_ref()
                .is_some_and(|id| id != &remote_thread)
            {
                return Err(Error::Conflict(
                    "Reply receipt changed remote thread identity".into(),
                ));
            }
            t.forge_thread_id = Some(remote_thread);
            tx.comments.update_thread(&threads::Model::from(&t)).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn get_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
    ) -> Result<Option<ForgeChange>> {
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let Some(receipt) = self
            .database
            .mutations
            .get_result(&session.context_id, &actor.id, &guard.request_key)
            .await?
        else {
            return Ok(None);
        };
        if receipt.method != input.method() || receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict(
                "Retry key belongs to a different remote change".into(),
            ));
        }
        Ok(Some(
            self.get_change_from_receipt(&self.database, &receipt, input)
                .await?,
        ))
    }

    pub(crate) async fn prepare_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
    ) -> Result<(ForgeChange, bool)> {
        let actor = self.auth.validate_requester(actor).await?;
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let tx = self.database.begin().await?;
        if self
            .begin_mutation::<ChangeReceipt>(
                &tx,
                &session.context_id,
                &actor.id,
                input.method(),
                Some(&guard),
                true,
            )
            .await?
            .is_some()
        {
            let receipt = tx
                .mutations
                .get_result(&session.context_id, &actor.id, &guard.request_key)
                .await?
                .ok_or_else(|| Error::NotFound("Remote acknowledgment".into()))?;
            let saved = self.get_change_from_receipt(&tx, &receipt, input).await?;
            tx.commit().await?;
            return Ok((saved, false));
        }
        let (_, object_id) = input.object();
        if tx
            .mutations
            .get_pending(&session.context_id, Some(&object_id))
            .await?
        {
            return Err(Error::Conflict(
                "This object has an unresolved remote change; retry its saved intent first".into(),
            ));
        }
        let context = self.load_context(&tx, &session.context_id).await?;
        if !context.key.is_forge_backed() {
            return Err(invalid("Remote changes require a PR context"));
        }
        let original = match input {
            ForgeChangeInput::Edit { comment_id, .. } | ForgeChangeInput::Delete { comment_id } => {
                if let ForgeChangeInput::Edit { body, .. } = input {
                    crate::models::Comment::validate_body(body)?;
                }
                let row = tx
                    .comments
                    .get_comment_by_id(&session.context_id, comment_id)
                    .await?
                    .ok_or_else(|| Error::NotFound("Comment".into()))?;
                let comment = Self::comment_from_db_model(row, &tx).await?;
                guard.validate_object(crate::models::ObjectKind::Comment, comment.id, &comment)?;
                if comment.origin_session_id.is_some() && comment.owner_id != Some(actor.id) {
                    return Err(invalid("Only the draft owner may change this feedback"));
                }
                if comment.visibility == "draft" {
                    self.require_active_session(&tx, session_id).await?;
                }
                ForgeChangeObject::Comment(Box::new(comment))
            }
            ForgeChangeInput::Resolve { thread_id, .. } => {
                let thread = self
                    .load_thread(&tx, thread_id, &session.context_id)
                    .await?;
                if matches!(thread.anchor, crate::models::Anchor::General) {
                    return Err(invalid("GitHub only supports resolving review threads"));
                }
                guard.validate_object(crate::models::ObjectKind::Thread, thread.id, &thread)?;
                ForgeChangeObject::Thread(Box::new(thread))
            }
        };
        let saved = ForgeChange {
            input: input.clone(),
            original,
            result: None,
            rejection: None,
            deleted_remote: None,
        };
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        tx.mutations
            .insert_result(&mutation_receipts::Model {
                context_id: session.context_id,
                actor_id: actor.id,
                request_key: guard.request_key,
                method: input.method().into(),
                request_hash: guard.payload_hash,
                result_json: serde_json::to_string(&ChangeReceipt::default())?,
                version,
                object_id: Some(object_id),
                pending: true,
            })
            .await?;
        tx.commit().await?;
        Ok((saved, true))
    }

    pub(crate) async fn finish_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
        mut result: Option<ForgeChangeObject>,
        rejection: Option<String>,
    ) -> Result<ForgeChange> {
        if result.is_none() && rejection.is_none() {
            return Err(invalid(
                "A remote change requires confirmation or definite rejection",
            ));
        }
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let tx = self.database.begin().await?;
        tx.review_contexts
            .lock_for_write(&session.context_id)
            .await?;
        let receipt = tx
            .mutations
            .get_result(&session.context_id, &actor.id, &guard.request_key)
            .await?
            .ok_or_else(|| Error::NotFound("Saved remote intent".into()))?;
        if receipt.method != input.method() || receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict("Remote intent changed".into()));
        }
        let mut saved = self.get_change_from_receipt(&tx, &receipt, input).await?;
        if saved.result.is_some() || saved.rejection.is_some() {
            tx.commit().await?;
            return Ok(saved);
        }
        if let Some(ForgeChangeObject::Thread(updated)) = &result {
            let mut current = self
                .load_thread(&tx, &updated.id, &session.context_id)
                .await?;
            current.state = updated.state.clone();
            current.updated_at = updated.updated_at.clone();
            current.resolved_at = updated.resolved_at.clone();
            current.resolved_by = updated.resolved_by.clone();
            current.updated_by = updated.updated_by.clone();
            result = Some(ForgeChangeObject::Thread(Box::new(current)));
        }
        match &result {
            Some(ForgeChangeObject::Comment(comment)) => {
                tx.comments
                    .update_comment(&comments::Model::from(comment.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Thread(thread)) => {
                tx.comments
                    .update_thread(&threads::Model::from(thread.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Deleted { comment_id }) => {
                let thread_id = match &saved.original {
                    ForgeChangeObject::Comment(comment) if comment.id == *comment_id => {
                        comment.thread_id
                    }
                    _ => return Err(invalid("Deletion receipt names another comment")),
                };
                tx.comments
                    .delete_remote_comment(&session.context_id, comment_id)
                    .await?;
                tx.comments
                    .delete_empty_thread(&session.context_id, &thread_id)
                    .await?;
                if let ForgeChangeObject::Comment(comment) = &saved.original {
                    saved.deleted_remote = comment
                        .forge_object_kind
                        .clone()
                        .zip(comment.forge_comment_id.clone());
                }
                // Keep only the retry receipt, not a copy of the deleted text.
                saved.original = ForgeChangeObject::Deleted {
                    comment_id: *comment_id,
                };
            }
            None => {}
        }
        saved.result = result;
        saved.rejection = rejection;
        tx.review_contexts
            .advance_revision(&session.context_id, None)
            .await?;
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        if !tx
            .mutations
            .update_result(
                &receipt,
                &serde_json::to_string(&ChangeReceipt {
                    rejection: saved.rejection.clone(),
                    deleted_remote: saved.deleted_remote.clone(),
                })?,
                version,
                false,
            )
            .await?
        {
            return Err(Error::Conflict("Remote receipt changed".into()));
        }
        tx.commit().await?;
        Ok(saved)
    }

    pub(crate) async fn get_posting_anchor(
        &self,
        session: &crate::models::Session,
        thread: &crate::models::Thread,
    ) -> Result<crate::models::Anchor> {
        let context = self.get_context_by_id(&session.context_id).await?;
        let crate::models::ContextKey::ForgeBacked {
            repository_url,
            number,
        } = &context.key
        else {
            return Err(invalid("Posting requires a PR context"));
        };
        self.sources
            .get_posting_anchor(
                &context.paths,
                crate::models::ReviewTarget::PullRequest {
                    repository_url: repository_url.as_str().into(),
                    number: *number,
                },
                crate::models::ReviewComparison::PullRequest {
                    base_commit: session
                        .commits
                        .base
                        .clone()
                        .ok_or_else(|| invalid("PR base unavailable"))?,
                    head_commit: session
                        .commits
                        .head
                        .clone()
                        .ok_or_else(|| invalid("PR head unavailable"))?,
                },
                &thread.anchor,
                thread.evidence.as_ref(),
                thread.current_path.as_deref(),
            )
            .await
    }

    pub(crate) async fn synchronize_pr_comparison(&self, observed: &Context) -> Result<Context> {
        if !observed.key.is_forge_backed()
            || observed.forge_state != Some(crate::database::types::ForgeState::Open)
        {
            return Ok(observed.clone());
        }
        let (base, head) = observed
            .refreshed_base
            .as_deref()
            .zip(observed.refreshed_head.as_deref())
            .ok_or_else(|| invalid("PR observation has no complete comparison"))?;
        let tx = self.database.begin().await?;
        let context = self.load_context(&tx, &observed.id).await?;
        if context.revision != observed.revision {
            return Err(Error::Conflict(
                "PR changed during synchronization; refresh again".into(),
            ));
        }
        if let Some(active) = tx.sessions.get_active(&context.id).await?
            && ["unsubmitted", "draft_failed"].contains(&active.submission_status.as_str())
            && (active.base_commit.as_deref() != Some(base)
                || active.head_commit.as_deref() != Some(head))
            && (!tx
                .review_contexts
                .advance_revision(&context.id, Some(context.revision))
                .await?
                || !tx
                    .sessions
                    .update_pr_comparison(&active, base, head)
                    .await?)
        {
            return Err(Error::Conflict(
                "PR review changed during synchronization".into(),
            ));
        }
        tx.commit().await?;
        self.get_context_by_id(&observed.id).await
    }

    // Private -----------------------------------------------------------------

    // Contexts ----------------------------------------------------------------

    async fn load_context<D: ConnectionTrait + TransactionTrait>(
        &self,
        conn: &db::Database<D>,
        id: &uuid::Uuid,
    ) -> Result<Context> {
        let row = conn
            .review_contexts
            .get_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?;
        Self::context_from_db_model(row, conn).await
    }

    async fn lock_context(&self, tx: &db::Transaction, id: &uuid::Uuid) -> Result<()> {
        if !tx.review_contexts.advance_revision(id, None).await? {
            return Err(Error::NotFound("Context".into()));
        }
        Ok(())
    }

    /// Check already-loaded metadata without querying the database inside a write transaction.
    async fn validate_local_context(context: &Context) -> Result<()> {
        if let ContextKey::Local {
            location: LocalLocation::Branch { git_dir, branch },
        } = &context.key
        {
            let root = context
                .paths
                .first()
                .ok_or_else(|| invalid("Local review has no working tree location"))?;
            let (_, actual_git_dir, actual_branch) =
                super::workspace::get_git_location(root).await?;
            if *git_dir != actual_git_dir || *branch != actual_branch {
                return Err(Error::Conflict(
                    "The selected branch or worktree changed; select its review before writing"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    async fn context_from_db_model<D: ConnectionTrait + TransactionTrait>(
        row: review_contexts::Model,
        conn: &db::Database<D>,
    ) -> Result<Context> {
        let paths = conn
            .workspace
            .get_paths(&row.id)
            .await?
            .into_iter()
            .map(|p| p.path)
            .collect();
        Context::from_db_model(row, paths)
    }

    // Reviews and sessions ----------------------------------------------------

    async fn require_active_session(
        &self,
        tx: &db::Transaction,
        id: &uuid::Uuid,
    ) -> Result<Session> {
        let row = tx
            .sessions
            .get_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound("Session".into()))?;
        let s = Self::session_from_db_model(row, tx).await?;
        if s.state() != "active" {
            return Err(Error::Conflict(
                "Completed session metadata cannot be changed; start a new session".into(),
            ));
        }
        Ok(s)
    }

    async fn sessions_from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<sessions::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Session>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.creator_id);
            if let Some(id) = &r.completed_by_id {
                ids.insert(*id);
            }
        }
        let actors = load_actors(ids, db).await?;
        rows.into_iter()
            .map(|r| {
                Session::from_db_model(
                    r.clone(),
                    get_actor(&actors, &r.creator_id)?,
                    r.completed_by_id
                        .as_ref()
                        .map(|id| get_actor(&actors, id))
                        .transpose()?,
                )
            })
            .collect()
    }

    async fn session_from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: sessions::Model,
        db: &db::Database<C>,
    ) -> Result<Session> {
        Self::sessions_from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Session".into()))
    }

    // Threads -----------------------------------------------------------------

    async fn load_thread(
        &self,
        tx: &db::Transaction,
        id: &uuid::Uuid,
        context: &uuid::Uuid,
    ) -> Result<Thread> {
        let row = tx
            .comments
            .get_thread(context, id)
            .await?
            .ok_or_else(|| Error::NotFound("Thread in selected context".into()))?;
        Self::thread_from_db_model(row, tx).await
    }

    async fn threads_from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<threads::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Thread>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.author_id);
            if let Some(id) = &r.resolved_by_id {
                ids.insert(*id);
            }
            if let Some(id) = &r.updated_by_id {
                ids.insert(*id);
            }
        }
        let actors = load_actors(ids, db).await?;
        rows.into_iter()
            .map(|r| {
                Thread::from_db_model(
                    r.clone(),
                    get_actor(&actors, &r.author_id)?,
                    r.resolved_by_id
                        .as_ref()
                        .map(|id| get_actor(&actors, id))
                        .transpose()?,
                    r.updated_by_id
                        .as_ref()
                        .map(|id| get_actor(&actors, id))
                        .transpose()?,
                )
            })
            .collect()
    }

    async fn thread_from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: threads::Model,
        db: &db::Database<C>,
    ) -> Result<Thread> {
        Self::threads_from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Thread".into()))
    }

    // Comments and notes ------------------------------------------------------

    async fn comments_from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<comments::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Comment>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.author_id);
        }
        let actors = load_actors(ids, db).await?;
        rows.into_iter()
            .map(|r| Comment::from_db_model(r.clone(), get_actor(&actors, &r.author_id)?))
            .collect()
    }

    async fn comment_from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: comments::Model,
        db: &db::Database<C>,
    ) -> Result<Comment> {
        Self::comments_from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Comment".into()))
    }

    // Anchors and locations ---------------------------------------------------

    async fn apply_location_move(
        &self,
        tx: &db::Transaction,
        context: &Context,
        scope: &ReviewScope,
        previous: &str,
        current: &str,
    ) -> Result<()> {
        use std::path::Path;
        let ContextKey::Local {
            location: LocalLocation::Notes { path: original },
        } = &context.key
        else {
            return Err(invalid(
                "Only saved-file scopes and directory reviews have movable locations",
            ));
        };
        let previous = Self::get_notes_path(previous)?;
        let current = Self::get_notes_path(current)?;
        let known = context.current_notes_path.as_ref().unwrap_or(original);
        if previous == current || matches!(scope, ReviewScope::Review { .. }) && &previous != known
        {
            return Err(Error::Conflict(
                "Move does not name the current saved location".into(),
            ));
        }
        match std::fs::symlink_metadata(&previous) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => {
                return Err(Error::Conflict(
                    "Move source still exists; a copy is not a confirmed move".into(),
                ));
            }
        }
        let directory = match scope {
            ReviewScope::Notes { path } => {
                if Self::get_notes_path(path)? != previous || !Path::new(&current).is_file() {
                    return Err(invalid(
                        "Saved-file move requires its original file scope and an existing destination file",
                    ));
                }
                false
            }
            ReviewScope::Review { .. } => {
                if !Path::new(&current).is_dir() {
                    return Err(invalid(
                        "Directory move requires an existing destination directory",
                    ));
                }
                true
            }
        };
        if let Some(other) = tx.review_contexts.get_notes_for_path(&current).await?
            && other.id != context.id
            && (other.notes_path.as_deref() == Some(&current)
                || other.current_notes_path.as_deref() == Some(&current))
        {
            return Err(Error::Conflict(
                "Destination already belongs to another saved conversation".into(),
            ));
        }
        for row in tx.comments.get_threads(&context.id).await? {
            let mut thread = Self::thread_from_db_model(row, tx).await?;
            let path = match &thread.anchor {
                Anchor::File { path } | Anchor::Line { path, .. } => Some(path),
                Anchor::General => None,
            };
            let known_path = thread
                .current_path
                .as_ref()
                .or(thread.source_path.as_ref())
                .map(|path| {
                    if Path::new(path).is_absolute() {
                        std::path::PathBuf::from(path)
                    } else {
                        Path::new(known).join(path)
                    }
                })
                .or_else(|| {
                    path.map(|path| {
                        if directory || &previous != known {
                            Path::new(known).join(path)
                        } else {
                            previous.clone().into()
                        }
                    })
                });
            let destination = if directory {
                known_path.and_then(|path| {
                    path.strip_prefix(&previous)
                        .ok()
                        .map(|relative| Path::new(&current).join(relative))
                })
            } else if (known_path.is_some() || &previous == known)
                && known_path
                    .as_ref()
                    .is_none_or(|path| path == Path::new(&previous))
            {
                Some(current.clone().into())
            } else {
                None
            };
            if let Some(destination) = destination {
                thread.current_path = Some(destination.to_string_lossy().into_owned());
                tx.comments
                    .update_thread(&threads::Model::from(&thread))
                    .await?;
                tx.workspace
                    .insert_path(&workspace_paths::Model {
                        context_id: context.id,
                        path: destination.to_string_lossy().into_owned(),
                    })
                    .await?;
            }
        }
        if &previous == known {
            tx.review_contexts
                .set_notes_location(&context.id, &current)
                .await?;
        }
        tx.workspace
            .insert_path(&workspace_paths::Model {
                context_id: context.id,
                path: current,
            })
            .await?;
        Ok(())
    }

    async fn validate_anchor_evidence(
        &self,
        context: &Context,
        session: &Session,
        anchor: &Anchor,
        evidence: &AnchorEvidence,
        scope: Option<&ReviewScope>,
    ) -> Result<()> {
        use crate::database::types::SourceKind;
        evidence.validate()?;
        let (path, range, side) = match anchor {
            Anchor::Line {
                path,
                start,
                end,
                side,
            } => (path, Some((*start, *end)), side.as_str()),
            Anchor::File { path } => (path, None, "new"),
            Anchor::General => return Err(invalid("Review-level feedback has no file source")),
        };
        if range.is_none()
            && (!evidence.selected_text.is_empty()
                || !evidence.context_before.is_empty()
                || !evidence.context_after.is_empty())
        {
            return Err(invalid("File evidence has no line selection"));
        }
        let (kind, source_commit) = match &evidence.source {
            AnchorSource::Commit { sha, .. } => (SourceKind::Commit, Some(sha.clone())),
            AnchorSource::IndexFile { .. } => (SourceKind::IndexFile, None),
            AnchorSource::WorkingFile { .. } => (SourceKind::WorkingFile, None),
            AnchorSource::LocalFile { .. } => (SourceKind::LocalFile, None),
        };
        let expected_commit = if side == "old" {
            session.commits.base.as_deref()
        } else {
            session.commits.head.as_deref()
        };
        if kind == SourceKind::Commit {
            let matches = if range.is_some() {
                context.key.is_forge_backed() || source_commit.as_deref() == expected_commit
            } else {
                context.key.is_forge_backed()
                    || source_commit == session.commits.base
                    || source_commit == session.commits.head
            };
            if !matches {
                return Err(Error::Conflict(
                    "Anchor commit differs from the selected comparison".into(),
                ));
            }
        }
        let allowed = if context.key.is_notes() {
            kind == SourceKind::LocalFile && side == "new"
        } else if context.key.is_forge_backed()
            || side == "old"
            || session.comparison_kind == ComparisonKind::Branch
        {
            kind == SourceKind::Commit
        } else {
            matches!(
                (session.comparison_kind, kind),
                (ComparisonKind::Staged, SourceKind::IndexFile)
                    | (
                        ComparisonKind::Working,
                        SourceKind::WorkingFile | SourceKind::IndexFile
                    )
            ) || (range.is_none() && kind == SourceKind::Commit)
        };
        if !allowed {
            return Err(invalid(
                "Anchor source does not belong to the selected comparison",
            ));
        }
        if context.key.is_forge_backed() {
            return Ok(());
        }
        let (root, path) =
            if let Some(absolute) = Self::get_saved_source_location(context, anchor, scope)? {
                let root = absolute
                    .parent()
                    .ok_or_else(|| invalid("Notes file has no parent"))?
                    .to_path_buf();
                let name = absolute
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| invalid("Notes file has no name"))?
                    .to_owned();
                (root, name)
            } else {
                (
                    context
                        .paths
                        .first()
                        .map(std::path::PathBuf::from)
                        .ok_or_else(|| invalid("Local review has no working tree location"))?,
                    path.clone(),
                )
            };
        let check_working_index =
            session.comparison_kind == ComparisonKind::Working && kind == SourceKind::IndexFile;
        let (actual, bytes) = tokio::task::spawn_blocking(move || {
            let selected =
                super::workspace::get_source(&root, &path, kind, source_commit.as_deref())?;
            if check_working_index {
                let (_, working) =
                    super::workspace::get_source(&root, &path, SourceKind::WorkingFile, None)?;
                if working != selected.1 {
                    return Err(Error::Conflict(
                        "Working bytes differ from the selected staged source".into(),
                    ));
                }
            }
            Ok::<_, Error>(selected)
        })
        .await
        .map_err(|_| invalid("Source validation failed"))??;
        let captured = if let Some((start, end)) = range {
            AnchorEvidence::capture(actual, &bytes, start, end)?
        } else {
            AnchorEvidence::capture_file(actual, &bytes)?
        };
        if captured != *evidence {
            return Err(Error::Conflict("Saved source or selected evidence changed; keep the body and refresh before commenting".into()));
        }
        Ok(())
    }

    fn get_saved_source_location(
        context: &Context,
        anchor: &Anchor,
        scope: Option<&ReviewScope>,
    ) -> Result<Option<std::path::PathBuf>> {
        let ContextKey::Local {
            location: LocalLocation::Notes { path: original },
        } = &context.key
        else {
            return Ok(None);
        };
        let current = std::path::Path::new(context.current_notes_path.as_ref().unwrap_or(original));
        let root = if current.is_dir() {
            current
        } else {
            current
                .parent()
                .ok_or_else(|| invalid("Notes file has no parent"))?
        };
        if let Some(ReviewScope::Notes { path }) = scope {
            let selected = std::path::PathBuf::from(Self::get_notes_path(path)?);
            if let Anchor::File { path } | Anchor::Line { path, .. } = anchor {
                let relative = selected.strip_prefix(root).ok().and_then(|p| p.to_str());
                if relative != Some(path.as_str())
                    && selected.file_name().and_then(|s| s.to_str()) != Some(path.as_str())
                {
                    return Err(invalid("Anchor differs from its saved-file scope"));
                }
            }
            return Ok(Some(selected));
        }
        match anchor {
            Anchor::File { path } | Anchor::Line { path, .. } => Ok(Some(root.join(path))),
            Anchor::General => Ok(None),
        }
    }

    fn get_notes_path(path: &str) -> Result<String> {
        let path = std::path::Path::new(path);
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(invalid("Notes require an absolute location without .."));
        }
        // Resolve the nearest surviving ancestor so /var aliases remain stable after deletion.
        for ancestor in path.ancestors() {
            if let Ok(canonical) = ancestor.canonicalize() {
                let suffix = path
                    .strip_prefix(ancestor)
                    .map_err(|_| invalid("Invalid notes location"))?;
                return Ok(if suffix.as_os_str().is_empty() {
                    canonical
                } else {
                    canonical.join(suffix)
                }
                .to_string_lossy()
                .into_owned());
            }
        }
        Ok(path.to_string_lossy().into_owned())
    }

    // Forge synchronization and recovery --------------------------------------

    async fn validate_forge_evidence(
        &self,
        context: &Context,
        session: &Session,
        anchor: &Anchor,
        evidence: &AnchorEvidence,
    ) -> Result<()> {
        self.validate_anchor_evidence(context, session, anchor, evidence, None)
            .await?;
        let ContextKey::ForgeBacked {
            repository_url,
            number,
        } = &context.key
        else {
            return Err(invalid("Expected a PR context"));
        };
        // An open composer can still refer to verified bytes from before a PR refresh.
        // Verify that exact source; posting derives a separate current placement.
        let mut commits = session.commits.clone();
        if let AnchorSource::Commit { sha, .. } = &evidence.source {
            if matches!(anchor, Anchor::Line {side, ..} if side == "old") {
                commits.base = Some(sha.clone());
            } else {
                commits.head = Some(sha.clone());
            }
        }
        self.sources
            .get_anchor_evidence(
                &context.paths,
                ReviewTarget::PullRequest {
                    repository_url: repository_url.as_str().into(),
                    number: *number,
                },
                ReviewComparison::PullRequest {
                    base_commit: commits
                        .base
                        .clone()
                        .ok_or_else(|| invalid("PR base unavailable"))?,
                    head_commit: commits
                        .head
                        .clone()
                        .ok_or_else(|| invalid("PR head unavailable"))?,
                },
                anchor,
                evidence,
            )
            .await?;
        Ok(())
    }

    /// PR acknowledgments refer to the canonical cache. Only local retries retain historical responses.
    async fn get_receipt_result<
        T: serde::de::DeserializeOwned,
        D: ConnectionTrait + TransactionTrait,
    >(
        database: &db::Database<D>,
        receipt: &mutation_receipts::Model,
    ) -> Result<T> {
        let value: serde_json::Value = serde_json::from_str(&receipt.result_json)?;
        let value = if let Some(id) = value
            .get("cached_comment")
            .and_then(serde_json::Value::as_str)
        {
            let id = uuid::Uuid::parse_str(id)
                .map_err(|_| invalid("Invalid cached comment reference"))?;
            let row = database
                .comments
                .get_comment_by_id(&receipt.context_id, &id)
                .await?
                .ok_or_else(|| Error::NotFound("Comment".into()))?;
            serde_json::to_value(Self::comment_from_db_model(row, database).await?)?
        } else if let Some(ids) = value
            .get("cached_comments")
            .and_then(serde_json::Value::as_array)
        {
            let mut comments = Vec::with_capacity(ids.len());
            for id in ids {
                let id = uuid::Uuid::parse_str(
                    id.as_str()
                        .ok_or_else(|| invalid("Invalid cached comment reference"))?,
                )
                .map_err(|_| invalid("Invalid cached comment reference"))?;
                let row = database
                    .comments
                    .get_comment_by_id(&receipt.context_id, &id)
                    .await?
                    .ok_or_else(|| Error::NotFound("Promoted comment".into()))?;
                comments.push(Self::comment_from_db_model(row, database).await?);
            }
            serde_json::to_value(comments)?
        } else if let Some(id) = value
            .get("cached_thread")
            .and_then(serde_json::Value::as_str)
        {
            let id = uuid::Uuid::parse_str(id)
                .map_err(|_| invalid("Invalid cached thread reference"))?;
            let row = database
                .comments
                .get_thread(&receipt.context_id, &id)
                .await?
                .ok_or_else(|| Error::NotFound("Thread".into()))?;
            serde_json::to_value(Self::thread_from_db_model(row, database).await?)?
        } else if let Some(id) = value
            .get("cached_review")
            .and_then(serde_json::Value::as_str)
        {
            let id = uuid::Uuid::parse_str(id)
                .map_err(|_| invalid("Invalid cached review reference"))?;
            let row = database
                .sessions
                .get_by_id(&id)
                .await?
                .ok_or_else(|| Error::NotFound("Review".into()))?;
            serde_json::to_value(Self::session_from_db_model(row, database).await?)?
        } else {
            value
        };
        Ok(serde_json::from_value(value)?)
    }

    async fn get_saved_mutation<T: serde::de::DeserializeOwned>(
        &self,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        method: &str,
        guard: &MutationGuard,
    ) -> Result<Option<T>> {
        let Some(receipt) = self
            .database
            .mutations
            .get_result(context, actor, &guard.request_key)
            .await?
        else {
            return Ok(None);
        };
        if receipt.method != method || receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict(
                "Retry key belongs to another action or payload".into(),
            ));
        }
        Ok(Some(
            Self::get_receipt_result(&self.database, &receipt).await?,
        ))
    }

    async fn begin_mutation<T: serde::de::DeserializeOwned>(
        &self,
        tx: &db::Transaction,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        method: &str,
        guard: Option<&MutationGuard>,
        advance: bool,
    ) -> Result<Option<T>> {
        if !tx.review_contexts.lock_for_write(context).await? {
            return Err(Error::NotFound("Context".into()));
        }
        if let Some(guard) = guard {
            if guard.request_key.trim().is_empty()
                || guard.request_key.len() > 200
                || guard.payload_hash.is_empty()
            {
                return Err(invalid("Mutation requires a bounded nonempty request key"));
            }
            if let Some(receipt) = tx
                .mutations
                .get_result(context, actor, &guard.request_key)
                .await?
            {
                if receipt.method != method || receipt.request_hash != guard.payload_hash {
                    return Err(Error::Conflict(
                        "Retry key belongs to a different operation or payload".into(),
                    ));
                }
                return Ok(Some(Self::get_receipt_result(tx, &receipt).await?));
            }
        }
        if advance {
            if !tx
                .review_contexts
                .advance_revision(context, guard.and_then(MutationGuard::write_version))
                .await?
            {
                return Err(Error::Conflict(
                    "Review data changed; read its current version before editing".into(),
                ));
            }
        } else if let Some(expected) = guard.and_then(MutationGuard::write_version)
            && tx
                .review_contexts
                .get_by_id(context)
                .await?
                .is_none_or(|row| row.revision != expected)
        {
            return Err(Error::Conflict(
                "Review data changed; read its current version".into(),
            ));
        }
        Ok(None)
    }

    async fn record_mutation<T: serde::Serialize>(
        tx: &db::Transaction,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        method: &str,
        guard: Option<&MutationGuard>,
        result: &T,
    ) -> Result<()> {
        if let Some(guard) = guard {
            let context_row = tx
                .review_contexts
                .get_by_id(context)
                .await?
                .ok_or_else(|| Error::NotFound("Context".into()))?;
            let value = serde_json::to_value(result)?;
            let value = if context_row.kind == ContextKind::ForgeBacked {
                match method {
                    "add_comment" | "reply" | "edit_comment" => {
                        serde_json::json!({"cached_comment":value["id"]})
                    }
                    "resolve" => serde_json::json!({"cached_thread":value["id"]}),
                    "start_session" | "complete" => {
                        serde_json::json!({"cached_review":value["id"]})
                    }
                    "promote_notes" => {
                        serde_json::json!({"cached_comments":value.as_array().ok_or_else(|| invalid("Invalid promotion result"))?.iter().map(|comment| comment["id"].clone()).collect::<Vec<_>>()})
                    }
                    _ => value,
                }
            } else {
                value
            };
            tx.mutations
                .insert_result(&mutation_receipts::Model {
                    context_id: *context,
                    actor_id: *actor,
                    request_key: guard.request_key.clone(),
                    method: method.into(),
                    request_hash: guard.payload_hash.clone(),
                    result_json: serde_json::to_string(&value)?,
                    object_id: None,
                    pending: false,
                    version: context_row.revision,
                })
                .await?;
        }
        Ok(())
    }

    async fn get_change_from_receipt<D: sea_orm::ConnectionTrait + sea_orm::TransactionTrait>(
        &self,
        database: &crate::database::Database<D>,
        receipt: &mutation_receipts::Model,
        input: &ForgeChangeInput,
    ) -> Result<ForgeChange> {
        let saved: ChangeReceipt = serde_json::from_str(&receipt.result_json)?;
        let (_, object_id) = input.object();
        let original = if saved.rejection.is_some()
            || (!receipt.pending && matches!(input, ForgeChangeInput::Delete { .. }))
        {
            ForgeChangeObject::Deleted {
                comment_id: object_id,
            }
        } else {
            match input {
                ForgeChangeInput::Edit { .. } | ForgeChangeInput::Delete { .. } => {
                    let row = database
                        .comments
                        .get_comment_by_id(&receipt.context_id, &object_id)
                        .await?
                        .ok_or_else(|| Error::NotFound("Comment".into()))?;
                    ForgeChangeObject::Comment(Box::new(
                        Self::comment_from_db_model(row, database).await?,
                    ))
                }
                ForgeChangeInput::Resolve { .. } => {
                    let row = database
                        .comments
                        .get_thread(&receipt.context_id, &object_id)
                        .await?
                        .ok_or_else(|| Error::NotFound("Thread".into()))?;
                    ForgeChangeObject::Thread(Box::new(
                        Self::thread_from_db_model(row, database).await?,
                    ))
                }
            }
        };
        Ok(ForgeChange {
            input: input.clone(),
            result: (!receipt.pending && saved.rejection.is_none()).then(|| original.clone()),
            original,
            rejection: saved.rejection,
            deleted_remote: saved.deleted_remote,
        })
    }
}
