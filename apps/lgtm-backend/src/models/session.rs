//! Session identity and immutable reviewed commits with explicit lifecycle data.
use super::super::{
    database::{models::sessions, types::SessionState},
    error::{Result, invalid},
};
use super::{Actor, CommitContext};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: uuid::Uuid,
    pub context_id: uuid::Uuid,
    pub creator: Actor,
    pub commits: CommitContext,
    pub created_at: String,
    #[serde(flatten)]
    pub status: SessionStatus,
    pub forge_review_id: Option<String>,
    pub submission_status: String,
    pub submission_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", content = "completion", rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Completed(Box<CompletedSession>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletedSession {
    pub at: String,
    pub by: Actor,
    pub outcome: String,
    pub body: Option<String>,
    pub request_key: String,
    pub request_hash: String,
}

impl Session {
    pub fn state(&self) -> &'static str {
        match self.status {
            SessionStatus::Active => "active",
            SessionStatus::Completed(_) => "completed",
        }
    }

    pub fn completion(&self) -> Option<&CompletedSession> {
        match &self.status {
            SessionStatus::Completed(v) => Some(v),
            _ => None,
        }
    }

    pub fn from_db_model(
        row: sessions::Model,
        creator: Actor,
        completed_by: Option<Actor>,
    ) -> Result<Self> {
        let missing = || invalid("Completed session is missing its completion metadata");
        let status = match row.state {
            SessionState::Active => SessionStatus::Active,
            SessionState::Completed => SessionStatus::Completed(Box::new(CompletedSession {
                at: row.completed_at.ok_or_else(missing)?,
                by: completed_by.ok_or_else(missing)?,
                outcome: row.outcome.ok_or_else(missing)?,
                body: row.body,
                request_key: row.request_key.ok_or_else(missing)?,
                request_hash: row.request_hash.ok_or_else(missing)?,
            })),
        };
        Ok(Self {
            id: row.id,
            context_id: row.context_id,
            creator,
            commits: CommitContext {
                base: row.base_commit,
                head: row.head_commit,
            },
            created_at: row.created_at,
            status,
            forge_review_id: row.forge_review_id,
            submission_status: row.submission_status,
            submission_error: row.submission_error,
        })
    }
}

impl From<&Session> for sessions::Model {
    fn from(value: &Session) -> Self {
        let completion = value.completion();
        Self {
            id: value.id,
            context_id: value.context_id,
            creator_id: value.creator.id,
            state: if completion.is_some() {
                SessionState::Completed
            } else {
                SessionState::Active
            },
            base_commit: value.commits.base.clone(),
            head_commit: value.commits.head.clone(),
            created_at: value.created_at.clone(),
            completed_at: completion.map(|v| v.at.clone()),
            completed_by_id: completion.map(|v| v.by.id),
            outcome: completion.map(|v| v.outcome.clone()),
            body: completion.and_then(|v| v.body.clone()),
            request_key: completion.map(|v| v.request_key.clone()),
            request_hash: completion.map(|v| v.request_hash.clone()),
            forge_review_id: value.forge_review_id.clone(),
            submission_status: value.submission_status.clone(),
            submission_error: value.submission_error.clone(),
        }
    }
}
