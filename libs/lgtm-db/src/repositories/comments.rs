//! Repository for general, inline, range, and threaded reply comments.

use crate::{
    entities::{
        comment::{self, ActiveModel, Column, Entity, Model as Comment},
        common::SessionState,
        session,
    },
    error::{DbError, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

/// Parameters for creating a new local or draft comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewComment {
    /// Target `reviews.id`.
    pub review_id: Uuid,
    /// Owning `sessions.id`.
    pub session_id: Uuid,
    /// Comment author `identities.id`.
    pub author_id: Uuid,
    /// Root comment UUID when creating a reply (`None` for root comments).
    pub reply_to_id: Option<Uuid>,
    /// Relative repository file path (`None` for general review comments).
    pub file_path: Option<String>,
    /// Starting line of a multi-line range (`None` for single-line or general comments).
    pub start_line: Option<i32>,
    /// Target line or ending line of a multi-line range (`None` for general comments).
    pub line: Option<i32>,
    /// Commit SHA against which the comment is anchored.
    pub commit_sha: Option<String>,
    /// Markdown body text.
    pub body: String,
}

/// Partial update patch for an existing comment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateComment {
    /// Updated markdown body text.
    pub body: Option<String>,
    /// Updated thread resolution status (root comments).
    pub resolved: Option<bool>,
    /// Updated anchor outdated flag.
    pub outdated: Option<bool>,
    /// Newly assigned remote `forge_id`.
    pub forge_id: Option<String>,
}

/// Anchor shift applied to a comment when a review's head commit advances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentAnchorShift {
    /// Target `comments.id`.
    pub comment_id: Uuid,
    /// Shifted starting line of a multi-line range.
    pub start_line: Option<i32>,
    /// Shifted target or ending line.
    pub line: Option<i32>,
    /// New head commit SHA.
    pub commit_sha: Option<String>,
    /// Whether the anchor was invalidated by conflicting hunk edits.
    pub outdated: bool,
}

/// Parameters for importing or updating a remote forge comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpsertForgeComment {
    /// Target `reviews.id`.
    pub review_id: Uuid,
    /// Owning `sessions.id`.
    pub session_id: Uuid,
    /// Comment author `identities.id`.
    pub author_id: Uuid,
    /// Root comment UUID when importing a reply (`None` for root comments).
    pub reply_to_id: Option<Uuid>,
    /// Opaque remote comment identifier (`forge_id`).
    pub forge_id: String,
    /// Relative repository file path.
    pub file_path: Option<String>,
    /// Current starting line of a multi-line range.
    pub start_line: Option<i32>,
    /// Current target or ending line.
    pub line: Option<i32>,
    /// Current commit SHA anchor.
    pub commit_sha: Option<String>,
    /// Original starting line when created.
    pub original_start_line: Option<i32>,
    /// Original target or ending line when created.
    pub original_line: Option<i32>,
    /// Original commit SHA when created.
    pub original_commit_sha: Option<String>,
    /// Markdown body text.
    pub body: String,
    /// Remote thread resolution state.
    pub resolved: bool,
    /// Remote outdated state.
    pub outdated: bool,
}

/// Persistence operations for `comments` rows.
#[derive(Clone, Debug)]
pub struct CommentsRepository<C> {
    pool: Arc<C>,
}

impl<C: ConnectionTrait> CommentsRepository<C> {
    /// Construct a repository sharing the given connection or transaction handle.
    pub(crate) fn new(pool: Arc<C>) -> Self {
        Self { pool }
    }

    // Public ------------------------------------------------------------------

    /// Fetch a comment by its internal UUID primary key.
    pub async fn get_comment(&self, id: Uuid) -> Result<Option<Comment>> {
        Ok(Entity::find_by_id(id).one(self.pool.as_ref()).await?)
    }

    /// Fetch a comment by its global sequential number (`C<number>`).
    pub async fn get_comment_by_number(&self, number: i64) -> Result<Option<Comment>> {
        if number <= 0 {
            return Ok(None);
        }
        Ok(Entity::find()
            .filter(Column::Number.eq(number))
            .one(self.pool.as_ref())
            .await?)
    }

    /// List all comments belonging to `session_id`, ordered by sequential `number` ascending.
    pub async fn get_comments_by_session(&self, session_id: Uuid) -> Result<Vec<Comment>> {
        Ok(Entity::find()
            .filter(Column::SessionId.eq(session_id))
            .order_by_asc(Column::Number)
            .all(self.pool.as_ref())
            .await?)
    }

    /// List comments in `review_id` authored by `author_id` that have not yet been synced remotely (`forge_id IS NULL`).
    pub async fn get_unsynced_comments_by_review(
        &self,
        review_id: Uuid,
        author_id: Uuid,
    ) -> Result<Vec<Comment>> {
        Ok(Entity::find()
            .filter(Column::ReviewId.eq(review_id))
            .filter(Column::AuthorId.eq(author_id))
            .filter(Column::ForgeId.is_null())
            .order_by_asc(Column::Number)
            .all(self.pool.as_ref())
            .await?)
    }

    /// List visible comments for `review_id`: all comments in `Published` sessions plus `viewer_id`'s own `Draft` session comments.
    pub async fn get_comments_by_review(
        &self,
        review_id: Uuid,
        viewer_id: Option<Uuid>,
    ) -> Result<Vec<Comment>> {
        let sessions = session::Entity::find()
            .filter(session::Column::ReviewId.eq(review_id))
            .all(self.pool.as_ref())
            .await?;

        let visible_session_ids: HashSet<Uuid> = sessions
            .into_iter()
            .filter(|sess| {
                sess.state == SessionState::Published
                    || (sess.state == SessionState::Draft && Some(sess.author_id) == viewer_id)
            })
            .map(|sess| sess.id)
            .collect();

        if visible_session_ids.is_empty() {
            return Ok(Vec::new());
        }

        Ok(Entity::find()
            .filter(Column::ReviewId.eq(review_id))
            .filter(Column::SessionId.is_in(visible_session_ids))
            .order_by_asc(Column::Number)
            .all(self.pool.as_ref())
            .await?)
    }

    /// Create a new comment, validating single-level reply threading and copying initial coordinates into `original_*`.
    pub async fn create_comment(&self, input: NewComment) -> Result<Comment> {
        let body = Self::require_non_empty("comment body", &input.body)?;
        let file_path = Self::normalize_optional(input.file_path);
        let commit_sha = Self::normalize_optional(input.commit_sha);
        Self::validate_anchor(file_path.as_deref(), input.start_line, input.line)?;
        self.validate_session_review(input.session_id, input.review_id)
            .await?;
        self.validate_reply_target(input.review_id, input.reply_to_id)
            .await?;

        let number = self.next_comment_number().await?;
        let now = Utc::now();

        let active = comment::ActiveModel {
            id: Set(Uuid::new_v4()),
            number: Set(number),
            review_id: Set(input.review_id),
            session_id: Set(input.session_id),
            author_id: Set(input.author_id),
            reply_to_id: Set(input.reply_to_id),
            file_path: Set(file_path),
            start_line: Set(input.start_line),
            line: Set(input.line),
            commit_sha: Set(commit_sha.clone()),
            original_start_line: Set(input.start_line),
            original_line: Set(input.line),
            original_commit_sha: Set(commit_sha),
            body: Set(body.to_owned()),
            resolved: Set(false),
            outdated: Set(false),
            forge_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Apply a partial update (`body`, `resolved`, `outdated`, or `forge_id`) to an existing comment.
    pub async fn update_comment(&self, id: Uuid, patch: UpdateComment) -> Result<Comment> {
        let existing = self.require_comment(id).await?;
        let mut active: ActiveModel = existing.into();

        if let Some(body) = patch.body {
            let trimmed = Self::require_non_empty("comment body", &body)?;
            active.body = Set(trimmed.to_owned());
        }
        if let Some(resolved) = patch.resolved {
            active.resolved = Set(resolved);
        }
        if let Some(outdated) = patch.outdated {
            active.outdated = Set(outdated);
        }
        if let Some(forge_id) = patch.forge_id {
            let trimmed = Self::require_non_empty("comment forge_id", &forge_id)?;
            active.forge_id = Set(Some(trimmed.to_owned()));
        }
        active.updated_at = Set(Utc::now());

        active
            .update(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Batch-update current line anchors (`start_line`, `line`, `commit_sha`) and `outdated` status while preserving `original_*`.
    pub async fn update_comment_anchors(&self, shifts: &[CommentAnchorShift]) -> Result<()> {
        let now = Utc::now();
        for shift in shifts {
            let existing = self.require_comment(shift.comment_id).await?;
            Self::validate_anchor(existing.file_path.as_deref(), shift.start_line, shift.line)?;
            let commit_sha = Self::normalize_optional(shift.commit_sha.clone());
            let mut active: ActiveModel = existing.into();
            active.start_line = Set(shift.start_line);
            active.line = Set(shift.line);
            active.commit_sha = Set(commit_sha);
            active.outdated = Set(shift.outdated);
            active.updated_at = Set(now);
            active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err)?;
        }
        Ok(())
    }

    /// Insert or update a remote forge comment keyed by `(review_id, forge_id)`.
    pub async fn upsert_forge_comment(&self, input: UpsertForgeComment) -> Result<Comment> {
        let forge_id = Self::require_non_empty("comment forge_id", &input.forge_id)?;
        let body = Self::require_non_empty("comment body", &input.body)?;
        let file_path = Self::normalize_optional(input.file_path);
        let commit_sha = Self::normalize_optional(input.commit_sha);
        let original_commit_sha = Self::normalize_optional(input.original_commit_sha);
        Self::validate_anchor(file_path.as_deref(), input.start_line, input.line)?;
        self.validate_session_review(input.session_id, input.review_id)
            .await?;
        self.validate_reply_target(input.review_id, input.reply_to_id)
            .await?;

        let now = Utc::now();
        if let Some(existing) = Entity::find()
            .filter(Column::ReviewId.eq(input.review_id))
            .filter(Column::ForgeId.eq(forge_id))
            .one(self.pool.as_ref())
            .await?
        {
            let mut active: ActiveModel = existing.into();
            active.session_id = Set(input.session_id);
            active.author_id = Set(input.author_id);
            active.reply_to_id = Set(input.reply_to_id);
            active.file_path = Set(file_path);
            active.start_line = Set(input.start_line);
            active.line = Set(input.line);
            active.commit_sha = Set(commit_sha);
            if input.original_start_line.is_some() {
                active.original_start_line = Set(input.original_start_line);
            }
            if input.original_line.is_some() {
                active.original_line = Set(input.original_line);
            }
            if original_commit_sha.is_some() {
                active.original_commit_sha = Set(original_commit_sha);
            }
            active.body = Set(body.to_owned());
            active.resolved = Set(input.resolved);
            active.outdated = Set(input.outdated);
            active.updated_at = Set(now);
            return active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err);
        }

        let number = self.next_comment_number().await?;
        let active = comment::ActiveModel {
            id: Set(Uuid::new_v4()),
            number: Set(number),
            review_id: Set(input.review_id),
            session_id: Set(input.session_id),
            author_id: Set(input.author_id),
            reply_to_id: Set(input.reply_to_id),
            file_path: Set(file_path),
            start_line: Set(input.start_line),
            line: Set(input.line),
            commit_sha: Set(commit_sha.clone()),
            original_start_line: Set(input.original_start_line.or(input.start_line)),
            original_line: Set(input.original_line.or(input.line)),
            original_commit_sha: Set(original_commit_sha.or(commit_sha)),
            body: Set(body.to_owned()),
            resolved: Set(input.resolved),
            outdated: Set(input.outdated),
            forge_id: Set(Some(forge_id.to_owned())),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Delete a comment by UUID (cascading to replies when deleting a root comment).
    pub async fn delete_comment(&self, id: Uuid) -> Result<()> {
        let existing = self.require_comment(id).await?;
        let replies: Vec<Comment> = Entity::find()
            .filter(Column::ReplyToId.eq(existing.id))
            .all(self.pool.as_ref())
            .await?;
        for reply in replies {
            Entity::delete_by_id(reply.id)
                .exec(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err)?;
        }
        Entity::delete_by_id(existing.id)
            .exec(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)?;
        Ok(())
    }

    // Private -----------------------------------------------------------------

    async fn require_comment(&self, id: Uuid) -> Result<Comment> {
        self.get_comment(id)
            .await?
            .ok_or_else(|| DbError::not_found("comment", id.to_string()))
    }

    async fn next_comment_number(&self) -> Result<i64> {
        let latest = Entity::find()
            .order_by_desc(Column::Number)
            .one(self.pool.as_ref())
            .await?;
        Ok(latest.map_or(1, |row| row.number + 1))
    }

    async fn validate_session_review(&self, session_id: Uuid, review_id: Uuid) -> Result<()> {
        let sess = session::Entity::find_by_id(session_id)
            .one(self.pool.as_ref())
            .await?
            .ok_or_else(|| DbError::invalid_input(format!("session {session_id} not found")))?;
        if sess.review_id != review_id {
            return Err(DbError::invalid_input(
                "session_id does not belong to review_id",
            ));
        }
        Ok(())
    }

    async fn validate_reply_target(
        &self,
        review_id: Uuid,
        reply_to_id: Option<Uuid>,
    ) -> Result<()> {
        let Some(parent_id) = reply_to_id else {
            return Ok(());
        };
        let parent = Entity::find_by_id(parent_id)
            .one(self.pool.as_ref())
            .await?
            .ok_or_else(|| {
                DbError::invalid_input(format!("reply_to_id comment {parent_id} not found"))
            })?;
        if parent.review_id != review_id {
            return Err(DbError::invalid_input(
                "reply_to_id comment belongs to a different review",
            ));
        }
        if parent.reply_to_id.is_some() {
            return Err(DbError::invalid_input(
                "cannot reply to a reply; reply_to_id must reference a root comment",
            ));
        }
        Ok(())
    }

    fn validate_anchor(
        file_path: Option<&str>,
        start_line: Option<i32>,
        line: Option<i32>,
    ) -> Result<()> {
        if (start_line.is_some() || line.is_some()) && file_path.is_none() {
            return Err(DbError::invalid_input(
                "line-anchored comment requires a non-empty file_path",
            ));
        }
        if let Some(target_line) = line
            && target_line <= 0
        {
            return Err(DbError::invalid_input("line must be a positive integer"));
        }
        if let Some(start) = start_line {
            if start <= 0 {
                return Err(DbError::invalid_input(
                    "start_line must be a positive integer",
                ));
            }
            let Some(end) = line else {
                return Err(DbError::invalid_input(
                    "start_line requires an ending line number",
                ));
            };
            if start > end {
                return Err(DbError::invalid_input("start_line must not exceed line"));
            }
        }
        Ok(())
    }

    fn normalize_optional(value: Option<String>) -> Option<String> {
        value
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    }

    fn require_non_empty<'a>(field: &'static str, value: &'a str) -> Result<&'a str> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DbError::invalid_input(format!("{field} must not be empty")));
        }
        Ok(trimmed)
    }
}
