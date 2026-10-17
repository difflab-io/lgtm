//! Review application actions over an injected database client and request logger.
use super::super::database as db;
use super::super::{
    error::{Error, Result, invalid},
    models::*,
};
use super::AuthService;
use db::Database;
use db::models::*;
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Clone)]
pub struct ReviewService {
    database: Database,
    auth: AuthService,
    logger: tracing::Span,
}

impl ReviewService {
    pub fn new(database: Database, auth: AuthService, logger: tracing::Span) -> Self {
        Self {
            database,
            auth,
            logger,
        }
    }

    pub fn auth(&self) -> &AuthService {
        &self.auth
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub async fn context(&self, key: &ContextKey) -> Result<Option<Context>> {
        key.validate()?;
        match self
            .database
            .review_contexts
            .by_key(key.as_database_key())
            .await?
        {
            Some(row) => Ok(Some(Context::from_db_model(row, &self.database).await?)),
            None => Ok(None),
        }
    }

    async fn load_context<D: ConnectionTrait + TransactionTrait>(
        &self,
        conn: &db::Database<D>,
        id: &str,
    ) -> Result<Context> {
        let row = conn
            .review_contexts
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?;
        Context::from_db_model(row, conn).await
    }

    async fn context_by_id(&self, id: &str) -> Result<Context> {
        self.load_context(&self.database, id).await
    }

    pub async fn ensure_context(&self, key: ContextKey, path: Option<&str>) -> Result<Context> {
        key.validate()?;
        if path.is_some_and(|p| !std::path::Path::new(p).is_absolute()) {
            return Err(invalid("Workspace path must be canonical and absolute"));
        }
        let tx = self.database.begin().await?;
        if let Some(repo) = key.repository() {
            tx.workspace
                .insert_repository(&repositories::Model { id: repo.into() })
                .await?;
        }
        let seed = Context {
            id: id(),
            key: key.clone(),
            paths: vec![],
            revision: 0,
            last_refreshed_at: None,
            refreshed_head: None,
            refreshed_base: None,
            refresh_status: "never".into(),
            refresh_error: None,
        };
        tx.review_contexts
            .insert(&review_contexts::Model::from(&seed))
            .await?;
        let row = tx
            .review_contexts
            .by_key(key.as_database_key())
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?;
        if let Some(path) = path {
            tx.workspace
                .insert_path(&workspace_paths::Model {
                    context_id: row.id.clone(),
                    path: path.into(),
                })
                .await?;
        }
        let context = Context::from_db_model(row, &tx).await?;
        tx.commit().await?;
        Ok(context)
    }

    pub async fn comment(&self, context: &str, id: &str) -> Result<Comment> {
        let row = self
            .database
            .comments
            .get(context, id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment in selected context".into()))?;
        Comment::from_db_model(row, &self.database).await
    }

    pub async fn session(&self, id: &str) -> Result<Session> {
        let row = self
            .database
            .sessions
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id}")))?;
        Session::from_db_model(row, &self.database).await
    }

    pub async fn selected_session(
        &self,
        context: &str,
        id: Option<&str>,
        active_only: bool,
    ) -> Result<Session> {
        if let Some(id) = id {
            let s = self.session(id).await?;
            if s.context_id != context {
                return Err(invalid("Session belongs to another context"));
            }
            return Ok(s);
        }
        let mut rows = Session::from_db_models(
            self.database
                .sessions
                .list(Some(context), active_only)
                .await?,
            &self.database,
        )
        .await?;
        rows.sort_by(|a, b| {
            (a.state != "active", std::cmp::Reverse(&a.created_at), &a.id).cmp(&(
                b.state != "active",
                std::cmp::Reverse(&b.created_at),
                &b.id,
            ))
        });
        rows.into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("Session; run lgtm session start".into()))
    }

    pub async fn list_sessions(&self, filter: &SessionFilter) -> Result<Vec<(Context, Session)>> {
        let mut result = vec![];
        for session in Session::from_db_models(
            self.database.sessions.list(None, !filter.completed).await?,
            &self.database,
        )
        .await?
        {
            let context = self.context_by_id(&session.context_id).await?;
            if filter
                .repository
                .as_deref()
                .is_some_and(|r| context.key.repository() != Some(r))
            {
                continue;
            }
            if let Some(root) = &filter.path {
                let matches = match &context.key {
                    ContextKey::Notes { path } => within(path, root),
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

    pub async fn conversation(&self, context: &str, include_drafts: bool) -> Result<Conversation> {
        let all =
            Comment::from_db_models(self.database.comments.list(context).await?, &self.database)
                .await?;
        let visible: std::collections::HashSet<_> = all
            .iter()
            .filter(|c| c.position == 0 && (include_drafts || c.visibility == "published"))
            .map(|c| c.thread_id.clone())
            .collect();
        let comments = all
            .into_iter()
            .filter(|c| {
                visible.contains(&c.thread_id) && (include_drafts || c.visibility == "published")
            })
            .collect();
        let threads = Thread::from_db_models(
            self.database.comments.list_threads(context).await?,
            &self.database,
        )
        .await?
        .into_iter()
        .filter(|t| visible.contains(&t.id))
        .collect();
        let sessions = Session::from_db_models(
            self.database.sessions.list(Some(context), false).await?,
            &self.database,
        )
        .await?;
        Ok(Conversation {
            context: self.context_by_id(context).await?,
            sessions,
            threads,
            comments,
        })
    }

    async fn lock_context(&self, tx: &db::Transaction, id: &str) -> Result<()> {
        if !tx.review_contexts.advance_revision(id, None).await? {
            return Err(Error::NotFound("Context".into()));
        }
        Ok(())
    }

    async fn require_active_session(&self, tx: &db::Transaction, id: &str) -> Result<Session> {
        let row = tx
            .sessions
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound("Session".into()))?;
        let s = Session::from_db_model(row, tx).await?;
        if s.state != "active" {
            return Err(Error::Conflict(
                "Completed session metadata cannot be changed; start a new session".into(),
            ));
        }
        Ok(s)
    }

    async fn load_thread(&self, tx: &db::Transaction, id: &str, context: &str) -> Result<Thread> {
        let row = tx
            .comments
            .get_thread(context, id)
            .await?
            .ok_or_else(|| Error::NotFound("Thread in selected context".into()))?;
        Thread::from_db_model(row, tx).await
    }

    pub async fn start_session(
        &self,
        context: &Context,
        actor: &Actor,
        commits: CommitContext,
    ) -> Result<Session> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: start_session"));
        let actor = &self.auth.validate_requester(actor).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &context.id).await?;
        let row = tx.sessions.active(&context.id).await?;
        if let Some(row) = row {
            let s = Session::from_db_model(row, &tx).await?;
            tx.commit().await?;
            return Ok(s);
        }
        if matches!(context.key, ContextKey::PullRequest { .. }) {
            let fresh = self.load_context(&tx, &context.id).await?;
            if !["live", "stub"].contains(&fresh.refresh_status.as_str())
                || fresh.last_refreshed_at.is_none()
            {
                return Err(invalid(
                    "PR start requires a fresh complete forge fetch; configure a provider (or an explicit test fixture stub)",
                ));
            }
            let latest = tx.sessions.list(Some(&context.id), false).await?;
            if latest.iter().any(|s| {
                s.completed_at.as_ref().is_some_and(|time| {
                    fresh
                        .last_refreshed_at
                        .as_ref()
                        .is_none_or(|stamp| stamp <= time)
                })
            }) {
                return Err(invalid(
                    "PR start requires a new live refresh after the previous session",
                ));
            }
        }
        let s = Session {
            id: id(),
            context_id: context.id.clone(),
            creator: actor.clone(),
            state: "active".into(),
            commits,
            created_at: now(),
            completed_at: None,
            completed_by: None,
            outcome: None,
            body: None,
            forge_review_id: None,
            submission_status: if matches!(context.key, ContextKey::PullRequest { .. }) {
                "unsubmitted"
            } else {
                "local"
            }
            .into(),
            submission_error: None,
            request_key: None,
            request_hash: None,
        };
        tx.sessions.insert(&sessions::Model::from(&s)).await?;
        // Original anchors are immutable; only mark their current applicability.
        let rows = tx.comments.list_threads(&context.id).await?;
        for mut t in Thread::from_db_models(rows, &tx).await? {
            if let Anchor::Line { side, commit, .. } = &t.anchor {
                let current = if side == "old" {
                    &s.commits.base
                } else {
                    &s.commits.head
                };
                t.outdated = current.as_ref().is_none_or(|head| head != commit);
                tx.comments.update_thread(&threads::Model::from(&t)).await?;
            }
        }
        tx.commit().await?;
        Ok(s)
    }

    pub async fn add_comment(
        &self,
        session_id: &str,
        actor: &Actor,
        body: &str,
        anchor: Anchor,
    ) -> Result<Comment> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: add_comment"));
        let actor = &self.auth.validate_requester(actor).await?;
        Comment::validate_body(body)?;
        anchor.validate()?;
        let seed = self.session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let s = self.require_active_session(&tx, session_id).await?;
        if let Anchor::Line { side, commit, .. } = &anchor {
            let reviewed = if side == "old" {
                &s.commits.base
            } else {
                &s.commits.head
            };
            if reviewed.as_deref() != Some(commit.as_str()) {
                return Err(invalid(
                    "Line anchor must reference the session's available reviewed commit",
                ));
            }
        }
        let t = Thread {
            id: id(),
            context_id: s.context_id.clone(),
            origin_session_id: Some(s.id.clone()),
            author: actor.clone(),
            anchor,
            state: "open".into(),
            resolved_by: None,
            resolved_at: None,
            updated_by: Some(actor.clone()),
            updated_at: now(),
            forge_thread_id: None,
            outdated: false,
        };
        tx.comments.insert_thread(&threads::Model::from(&t)).await?;
        let c = local_comment(&s, &t, actor, body, 0);
        tx.comments.insert(&comments::Model::from(&c)).await?;
        tx.commit().await?;
        Ok(c)
    }

    pub async fn reply(
        &self,
        session_id: &str,
        thread_id: &str,
        actor: &Actor,
        body: &str,
    ) -> Result<Comment> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: reply"));
        let actor = &self.auth.validate_requester(actor).await?;
        Comment::validate_body(body)?;
        let seed = self.session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let s = self.require_active_session(&tx, session_id).await?;
        let t = self.load_thread(&tx, thread_id, &s.context_id).await?;
        let position = tx.comments.next_position(thread_id).await?;
        let c = local_comment(&s, &t, actor, body, position);
        tx.comments.insert(&comments::Model::from(&c)).await?;
        tx.commit().await?;
        Ok(c)
    }

    pub async fn resolve(
        &self,
        session_id: &str,
        thread_id: &str,
        actor: &Actor,
        resolved: bool,
    ) -> Result<Thread> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: resolve"));
        let actor = &self.auth.validate_requester(actor).await?;
        let seed = self.session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        self.require_active_session(&tx, session_id).await?;
        let mut t = self.load_thread(&tx, thread_id, &seed.context_id).await?;
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
        tx.commit().await?;
        Ok(t)
    }

    pub async fn complete(
        &self,
        context_id: &str,
        actor: &Actor,
        input: Completion,
        request_key: &str,
    ) -> Result<Session> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: complete"));
        let actor = &self.auth.validate_requester(actor).await?;
        if request_key.trim().is_empty() {
            return Err(invalid("Completion requires a nonempty retry key"));
        }
        let hash = format!(
            "{:x}",
            Sha256::digest(payload(&(input.clone(), &actor.id))?)
        );
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        let row = tx
            .sessions
            .completion_by_key(context_id, request_key)
            .await?;
        if let Some(row) = row {
            let s = Session::from_db_model(row, &tx).await?;
            if s.request_hash.as_deref() != Some(&hash) {
                return Err(Error::Conflict(
                    "Retry key was already used with different content or actor".into(),
                ));
            }
            tx.commit().await?;
            return Ok(s);
        }
        let row = tx
            .sessions
            .active(context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Active session".into()))?;
        let mut s = Session::from_db_model(row, &tx).await?;
        let context = self.load_context(&tx, context_id).await?;
        if context.key.kind() == "pr" {
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
        let rows = tx.comments.drafts_for_session(&s.id).await?;
        for mut c in Comment::from_db_models(rows, &tx).await? {
            c.visibility = "published".into();
            c.updated_at = now();
            tx.comments.update(&comments::Model::from(&c)).await?;
        }
        s.state = "completed".into();
        s.completed_at = Some(now());
        s.completed_by = Some(actor.clone());
        s.outcome = Some(input.outcome.unwrap_or_else(|| {
            if context.key.kind() == "notes" {
                "saved"
            } else {
                "ready-to-address"
            }
            .into()
        }));
        s.body = input.body;
        s.request_key = Some(request_key.into());
        s.request_hash = Some(hash);
        if input.submit {
            s.submission_status = "unsubmitted".into();
            s.submission_error = Some("STUB: no forge submission was performed".into());
        }
        tx.sessions.complete(&sessions::Model::from(&s)).await?;
        tx.commit().await?;
        Ok(s)
    }

    pub async fn record_refresh_failure(&self, context: &Context, message: &str) -> Result<()> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: record_refresh_failure"));
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &context.id).await?;
        let mut current = self.load_context(&tx, &context.id).await?;
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
        snapshot: ForgeSnapshot,
        stub: bool,
    ) -> Result<Context> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: apply_remote"));
        if !snapshot.complete || snapshot.head.is_empty() {
            return Err(invalid(
                "Incomplete forge fetch; all pages and reviewed head are required",
            ));
        }
        let tx = self.database.begin().await?;
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
                let actor = Actor::find_or_create(
                    &tx,
                    &c.author.name,
                    &c.author.email,
                    None,
                    Some(format!(
                        "forge:{}",
                        serde_json::to_string(&(
                            match &context.key {
                                ContextKey::PullRequest { host, .. } => host,
                                _ => unreachable!(),
                            },
                            &c.author.account_id
                        ))?
                    )),
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
                Thread::from_db_model(row, &tx).await?
            } else {
                let root = &remote_thread.comments[0];
                let existing = tx
                    .comments
                    .get_by_remote_id(&context.id, &root.kind, &root.id)
                    .await?;
                if let Some(row) = existing {
                    let c = Comment::from_db_model(row, &tx).await?;
                    self.load_thread(&tx, &c.thread_id, &context.id).await?
                } else {
                    let t = Thread {
                        id: id(),
                        context_id: context.id.clone(),
                        origin_session_id: None,
                        author: authors[author_index].clone(),
                        anchor: remote_thread.anchor.clone(),
                        state: "open".into(),
                        resolved_by: None,
                        resolved_at: None,
                        updated_by: None,
                        updated_at: now(),
                        forge_thread_id: Some(remote_thread.id.clone()),
                        outdated: false,
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
            if let Anchor::Line { commit, side, .. } = &thread.anchor {
                thread.outdated = Some(commit)
                    != if side == "old" {
                        snapshot.base.as_ref()
                    } else {
                        Some(&snapshot.head)
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
                    .get_by_remote_id(&context.id, &remote.kind, &remote.id)
                    .await?;
                let existing = row.is_some();
                let mut comment = if let Some(row) = row {
                    let c = Comment::from_db_model(row, &tx).await?;
                    if c.thread_id != thread.id {
                        return Err(Error::Conflict(
                            "Remote comment unexpectedly moved between threads".into(),
                        ));
                    }
                    if ["sending", "uncertain"].contains(&c.posting_state.as_str()) {
                        return Err(Error::Conflict(
                            "Pending remote write requires explicit reconciliation".into(),
                        ));
                    }
                    c
                } else {
                    let position = tx.comments.next_position(&thread.id).await?;
                    Comment {
                        id: id(),
                        context_id: context.id.clone(),
                        thread_id: thread.id.clone(),
                        origin_session_id: None,
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
                        posting_state: "confirmed".into(),
                        posting_error: None,
                        posting_request_key: None,
                        payload_hash: None,
                        remote_presence: "present".into(),
                    }
                };
                // Preserve logical author. Strip only the exact payload previously rendered by us.
                if !existing || comment.forge_body.as_deref() != Some(&remote.body) {
                    comment.body = remote.body.clone();
                }
                comment.forge_body = Some(remote.body.clone());
                comment.forge_updated_at = Some(remote.updated_at.clone());
                comment.forge_review_id = remote.review_id.clone();
                comment.remote_presence = "present".into();
                comment.updated_at = now();
                if existing {
                    tx.comments.update(&comments::Model::from(&comment)).await?;
                } else {
                    tx.comments.insert(&comments::Model::from(&comment)).await?;
                }
            }
        }
        let rows = tx.comments.list_remote(&context.id).await?;
        for mut c in Comment::from_db_models(rows, &tx).await? {
            if !seen.contains(&(
                c.forge_object_kind.clone().unwrap_or_default(),
                c.forge_comment_id.clone().unwrap_or_default(),
            )) {
                c.remote_presence = "missing".into();
                tx.comments.update(&comments::Model::from(&c)).await?;
            }
        }
        let mut current = context.clone();
        current.last_refreshed_at = Some(now());
        current.refreshed_head = Some(snapshot.head);
        current.refreshed_base = snapshot.base;
        current.refresh_status = if stub { "stub" } else { "live" }.into();
        current.refresh_error = None;
        tx.review_contexts
            .update_refresh(&review_contexts::Model::from(&current))
            .await?;
        tx.commit().await?;
        self.context_by_id(&context.id).await
    }

    pub async fn stage_post(
        &self,
        session_id: &str,
        comment_id: &str,
        request_key: &str,
        expected: &Comment,
        rendered: &str,
    ) -> Result<String> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: stage_post"));
        if request_key.trim().is_empty() {
            return Err(invalid("Posting requires a retry key"));
        }
        let seed = self.session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let session = self.require_active_session(&tx, session_id).await?;
        let row = tx
            .comments
            .get(&session.context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("Comment in selected context".into()))?;
        let mut c = Comment::from_db_model(row, &tx).await?;
        if c.origin_session_id.as_deref() != Some(session_id) || c.forge_comment_id.is_some() {
            return Err(invalid(
                "Only this review's unposted comments may be submitted",
            ));
        }
        if ["sending", "uncertain", "confirmed"].contains(&c.posting_state.as_str()) {
            return Err(Error::Conflict(
                "Comment may already have been sent; reconcile before retrying".into(),
            ));
        }
        let context = self.load_context(&tx, &session.context_id).await?;
        if context.key.kind() != "pr" {
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
        c.posting_state = "sending".into();
        c.posting_request_key = Some(request_key.into());
        c.payload_hash = Some(format!("{:x}", Sha256::digest(&rendered)));
        c.forge_body = Some(rendered.clone());
        tx.comments.update(&comments::Model::from(&c)).await?;
        tx.commit().await?;
        Ok(rendered)
    }

    pub async fn posting_uncertain(
        &self,
        context_id: &str,
        comment_id: &str,
        message: &str,
    ) -> Result<()> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: posting_uncertain"));
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        let row = tx
            .comments
            .get(context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("comments".into()))?;
        let mut c = Comment::from_db_model(row, &tx).await?;
        c.posting_state = "uncertain".into();
        c.posting_error = Some(message.into());
        tx.comments.update(&comments::Model::from(&c)).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Supply only a verified provider receipt, never a body/footer guess.
    pub async fn confirm_post(
        &self,
        context_id: &str,
        comment_id: &str,
        request_key: &str,
        receipt: PostReceipt,
    ) -> Result<()> {
        self.logger
            .in_scope(|| tracing::debug!("Review action: confirm_post"));
        if receipt.comment_id.is_empty()
            || !["review_comment", "conversation_comment", "review"]
                .contains(&receipt.kind.as_str())
        {
            return Err(invalid("Invalid forge receipt"));
        }
        let tx = self.database.begin().await?;
        self.lock_context(&tx, context_id).await?;
        let row = tx
            .comments
            .get(context_id, comment_id)
            .await?
            .ok_or_else(|| Error::NotFound("comments".into()))?;
        let mut c = Comment::from_db_model(row, &tx).await?;
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
        c.posting_state = "confirmed".into();
        c.posting_error = None;
        c.remote_presence = "present".into();
        tx.comments.update(&comments::Model::from(&c)).await?;
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

    pub fn export(conversation: &Conversation, session: &Session) -> String {
        let mut out = format!(
            "# {}\n\nSession: {} ({})\nCreator: {}\nReviewed base/head: {:?} / {:?}\nOutcome: {:?}\nSubmission: {}\nLast refresh: {:?} ({})\n\nConversation content and resolution are CURRENT, not a historical snapshot.\n",
            conversation.context.key.label(),
            session.id,
            session.state,
            session.creator.label(),
            session.commits.base,
            session.commits.head,
            session.outcome,
            session.submission_status,
            conversation.context.last_refreshed_at,
            conversation.context.refresh_status
        );
        if let Some(body) = &session.body {
            out.push_str(&format!("\n{body}\n"));
        }
        let historical = session.state == "completed";
        for t in &conversation.threads {
            let all: Vec<_> = conversation
                .comments
                .iter()
                .filter(|c| c.thread_id == t.id)
                .collect();
            if historical
                && !all
                    .iter()
                    .any(|c| c.origin_session_id.as_deref() == Some(&session.id))
            {
                continue;
            }
            out.push_str(&format!(
                "\n## Thread {} [{}]{}\n\nAnchor: {:?}\nAuthor: {}\nResolver: {:?}\n",
                t.id,
                t.state,
                if t.outdated { " (outdated anchor)" } else { "" },
                t.anchor,
                t.author.label(),
                t.resolved_by.as_ref().map(Actor::label)
            ));
            for c in all {
                out.push_str(&format!("\n### {} — {}\n\nComment: {} | origin session: {:?} | {} / {} | forge: {:?}/{:?} | remote: {}\n\n{}\n",if c.position==0 {"Comment"} else {"Reply"},c.author.label(),c.id,c.origin_session_id,c.visibility,c.posting_state,c.forge_object_kind,c.forge_comment_id,c.remote_presence,c.body));
            }
        }
        out
    }
}

fn payload<T: Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_string(v)?)
}

fn local_comment(s: &Session, t: &Thread, actor: &Actor, body: &str, position: i64) -> Comment {
    Comment {
        id: id(),
        context_id: s.context_id.clone(),
        thread_id: t.id.clone(),
        origin_session_id: Some(s.id.clone()),
        author: actor.clone(),
        body: body.into(),
        position,
        visibility: "draft".into(),
        created_at: now(),
        updated_at: now(),
        forge_comment_id: None,
        forge_object_kind: None,
        forge_review_id: None,
        forge_body: None,
        forge_updated_at: None,
        posting_account_id: None,
        posting_state: "unposted".into(),
        posting_error: None,
        posting_request_key: None,
        payload_hash: None,
        remote_presence: "local".into(),
    }
}
