//! Repository for author-scoped draft and published review sessions.

use crate::{
    entities::{
        common::{ReviewOutcome, SessionState},
        session::{self, ActiveModel, Column, Entity, Model as Session},
    },
    error::{DbError, Result},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::sync::Arc;
use uuid::Uuid;

/// Parameters for importing or updating a remote forge review session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpsertForgeSession {
    /// Target `reviews.id`.
    pub review_id: Uuid,
    /// Session author `identities.id`.
    pub author_id: Uuid,
    /// Opaque remote review identifier (`forge_id`).
    pub forge_id: String,
    /// Session state (`Draft` or `Published`).
    pub state: SessionState,
    /// Session outcome (`None` when `Draft`, required when `Published`).
    pub outcome: Option<ReviewOutcome>,
    /// Optional top-level summary markdown body.
    pub body: Option<String>,
    /// Publication timestamp (`None` when `Draft`).
    pub published_at: Option<DateTime<Utc>>,
}

/// Persistence operations for `sessions` rows.
#[derive(Clone, Debug)]
pub struct SessionsRepository<C> {
    pool: Arc<C>,
}

impl<C: ConnectionTrait> SessionsRepository<C> {
    /// Construct a repository sharing the given connection or transaction handle.
    pub(crate) fn new(pool: Arc<C>) -> Self {
        Self { pool }
    }

    // Public ------------------------------------------------------------------

    /// Fetch a session by its internal UUID primary key.
    pub async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        Ok(Entity::find_by_id(id).one(self.pool.as_ref()).await?)
    }

    /// Fetch the active `Draft` session for `(review_id, author_id)`, if one exists.
    pub async fn get_draft_session(
        &self,
        review_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<Session>> {
        Ok(Entity::find()
            .filter(Column::ReviewId.eq(review_id))
            .filter(Column::AuthorId.eq(author_id))
            .filter(Column::State.eq(SessionState::Draft))
            .one(self.pool.as_ref())
            .await?)
    }

    /// Return the existing `Draft` session for `(review_id, author_id)` or create a new one.
    pub async fn get_or_create_draft_session(
        &self,
        review_id: Uuid,
        author_id: Uuid,
        body: Option<String>,
    ) -> Result<Session> {
        let normalized_body = Self::normalize_optional(body);
        if let Some(existing) = self.get_draft_session(review_id, author_id).await? {
            if let Some(next_body) = normalized_body
                && existing.body.as_deref() != Some(next_body.as_str())
            {
                let mut active: ActiveModel = existing.into();
                active.body = Set(Some(next_body));
                active.updated_at = Set(Utc::now());
                return active
                    .update(self.pool.as_ref())
                    .await
                    .map_err(DbError::from_db_err);
            }
            return Ok(existing);
        }

        let number = self.next_session_number(review_id).await?;
        let now = Utc::now();
        let active = session::ActiveModel {
            id: Set(Uuid::new_v4()),
            review_id: Set(review_id),
            author_id: Set(author_id),
            number: Set(number),
            state: Set(SessionState::Draft),
            outcome: Set(None),
            body: Set(normalized_body),
            forge_id: Set(None),
            published_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// List all sessions belonging to `review_id` ordered by sequential `number` ascending.
    pub async fn get_sessions_by_review(&self, review_id: Uuid) -> Result<Vec<Session>> {
        Ok(Entity::find()
            .filter(Column::ReviewId.eq(review_id))
            .order_by_asc(Column::Number)
            .all(self.pool.as_ref())
            .await?)
    }

    /// Transition a `Draft` session to `Published` with the given `outcome`, `body`, and optional `forge_id`.
    pub async fn publish_session(
        &self,
        session_id: Uuid,
        outcome: ReviewOutcome,
        body: Option<String>,
        forge_id: Option<String>,
    ) -> Result<Session> {
        let existing = self
            .get_session(session_id)
            .await?
            .ok_or_else(|| DbError::not_found("session", session_id.to_string()))?;

        if existing.state == SessionState::Published {
            return Err(DbError::conflict(format!(
                "session {} is already published",
                session_id
            )));
        }

        let now = Utc::now();
        let next_body = Self::normalize_optional(body).or(existing.body.clone());
        let next_forge_id = Self::normalize_optional(forge_id).or(existing.forge_id.clone());

        let mut active: ActiveModel = existing.into();
        active.state = Set(SessionState::Published);
        active.outcome = Set(Some(outcome));
        active.body = Set(next_body);
        active.forge_id = Set(next_forge_id);
        active.published_at = Set(Some(now));
        active.updated_at = Set(now);
        active
            .update(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Insert or update a remote forge session keyed by `(review_id, forge_id)`.
    pub async fn upsert_forge_session(&self, input: UpsertForgeSession) -> Result<Session> {
        let forge_id = input.forge_id.trim();
        if forge_id.is_empty() {
            return Err(DbError::invalid_input("session forge_id must not be empty"));
        }

        let now = Utc::now();
        let (outcome, published_at) = match input.state {
            SessionState::Draft => {
                if input.outcome.is_some() || input.published_at.is_some() {
                    return Err(DbError::invalid_input(
                        "Draft session must not specify outcome or published_at",
                    ));
                }
                (None, None)
            }
            SessionState::Published => {
                let outcome = input.outcome.ok_or_else(|| {
                    DbError::invalid_input("Published session requires a ReviewOutcome")
                })?;
                (Some(outcome), Some(input.published_at.unwrap_or(now)))
            }
        };
        let body = Self::normalize_optional(input.body);

        let existing = if let Some(by_forge) = Entity::find()
            .filter(Column::ReviewId.eq(input.review_id))
            .filter(Column::ForgeId.eq(forge_id))
            .one(self.pool.as_ref())
            .await?
        {
            Some(by_forge)
        } else if input.state == SessionState::Draft {
            self.get_draft_session(input.review_id, input.author_id)
                .await?
        } else {
            None
        };

        if let Some(existing) = existing {
            let mut active: ActiveModel = existing.into();
            active.author_id = Set(input.author_id);
            active.state = Set(input.state);
            active.outcome = Set(outcome);
            active.body = Set(body);
            active.forge_id = Set(Some(forge_id.to_owned()));
            active.published_at = Set(published_at);
            active.updated_at = Set(now);
            return active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err);
        }

        let number = self.next_session_number(input.review_id).await?;
        let active = session::ActiveModel {
            id: Set(Uuid::new_v4()),
            review_id: Set(input.review_id),
            author_id: Set(input.author_id),
            number: Set(number),
            state: Set(input.state),
            outcome: Set(outcome),
            body: Set(body),
            forge_id: Set(Some(forge_id.to_owned())),
            published_at: Set(published_at),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    // Private -----------------------------------------------------------------

    async fn next_session_number(&self, review_id: Uuid) -> Result<i64> {
        let latest = Entity::find()
            .filter(Column::ReviewId.eq(review_id))
            .order_by_desc(Column::Number)
            .one(self.pool.as_ref())
            .await?;
        Ok(latest.map_or(1, |row| row.number + 1))
    }

    fn normalize_optional(value: Option<String>) -> Option<String> {
        value
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    }
}
