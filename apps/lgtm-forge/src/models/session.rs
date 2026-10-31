//! Domain model for review sessions (draft and published comment batches).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    database,
    error::{Error, Result},
    models::{Identity, ReviewOutcome, comment::format_body_with_disclosure},
    proto::lgtm::v1::{
        IdentitySummary, ReviewOutcome as ProtoReviewOutcome, SessionState as ProtoSessionState,
        SessionSummary,
    },
};

/// Lifecycle state of a review session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Draft,
    Published,
}

/// Application session entity representing a draft or published batch of review comments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: Uuid,
    pub review_id: Uuid,
    pub number: u64,
    pub author: Identity,
    pub state: SessionState,
    pub outcome: Option<ReviewOutcome>,
    pub body: Option<String>,
    pub forge_id: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

impl Session {
    /// Convert a persisted database session row into a domain `Session`.
    pub fn from_db(model: database::Session, author: Identity) -> Result<Self> {
        let number = u64::try_from(model.number)
            .map_err(|_| Error::Invalid("session.number must be positive".to_owned()))?;
        let state = match model.state {
            database::SessionState::Draft => SessionState::Draft,
            database::SessionState::Published => SessionState::Published,
        };
        let outcome = model.outcome.map(ReviewOutcome::from);
        let published_at = model.published_at.map(|dt| dt.with_timezone(&Utc));
        Ok(Self {
            id: model.id,
            review_id: model.review_id,
            number,
            author,
            state,
            outcome,
            body: model.body,
            forge_id: model.forge_id,
            published_at,
        })
    }

    // Public ------------------------------------------------------------------

    /// Format an optional top-level review session summary body for remote export, including AI disclosure when authored by an `Agent`.
    pub fn format_export_body(&self, override_body: Option<&str>) -> Option<String> {
        let raw = override_body.or(self.body.as_deref()).unwrap_or("");
        let formatted = format_body_with_disclosure(raw, &self.author);
        if formatted.is_empty() {
            None
        } else {
            Some(formatted)
        }
    }
}

impl From<SessionState> for database::SessionState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Draft => Self::Draft,
            SessionState::Published => Self::Published,
        }
    }
}

impl From<SessionState> for ProtoSessionState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Draft => Self::SESSION_STATE_DRAFT,
            SessionState::Published => Self::SESSION_STATE_PUBLISHED,
        }
    }
}

impl From<Session> for SessionSummary {
    fn from(session: Session) -> Self {
        let outcome = session
            .outcome
            .map(ProtoReviewOutcome::from)
            .unwrap_or(ProtoReviewOutcome::REVIEW_OUTCOME_UNSPECIFIED);
        Self {
            id: session.id.to_string(),
            review_id: session.review_id.to_string(),
            number: session.number,
            author: Some(IdentitySummary::from(session.author)).into(),
            state: ProtoSessionState::from(session.state).into(),
            outcome: outcome.into(),
            body: session.body,
            forge_id: session.forge_id,
            published_at: session.published_at.map(|dt| dt.to_rfc3339()),
            ..Default::default()
        }
    }
}
