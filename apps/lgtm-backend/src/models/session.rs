use super::super::database::models::sessions;
use super::super::{
    database as db,
    error::{Error, Result},
};
use super::identities::IdentityMap;
use super::{Actor, CommitContext};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub context_id: String,
    pub creator: Actor,
    pub state: String,
    pub commits: CommitContext,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub completed_by: Option<Actor>,
    pub outcome: Option<String>,
    pub body: Option<String>,
    pub forge_review_id: Option<String>,
    pub submission_status: String,
    pub submission_error: Option<String>,
    pub request_key: Option<String>,
    pub request_hash: Option<String>,
}

impl From<&Session> for sessions::Model {
    fn from(v: &Session) -> Self {
        Self {
            id: v.id.clone(),
            context_id: v.context_id.clone(),
            creator_id: v.creator.id.clone(),
            state: v.state.clone(),
            base_commit: v.commits.base.clone(),
            head_commit: v.commits.head.clone(),
            created_at: v.created_at.clone(),
            completed_at: v.completed_at.clone(),
            completed_by_id: v.completed_by.as_ref().map(|a| a.id.clone()),
            outcome: v.outcome.clone(),
            body: v.body.clone(),
            forge_review_id: v.forge_review_id.clone(),
            submission_status: v.submission_status.clone(),
            submission_error: v.submission_error.clone(),
            request_key: v.request_key.clone(),
            request_hash: v.request_hash.clone(),
        }
    }
}

impl Session {
    fn with_identities(v: sessions::Model, actors: &IdentityMap) -> Result<Self> {
        Ok(Session {
            id: v.id,
            context_id: v.context_id,
            state: v.state,
            created_at: v.created_at,
            completed_at: v.completed_at,
            outcome: v.outcome,
            body: v.body,
            forge_review_id: v.forge_review_id,
            submission_status: v.submission_status,
            submission_error: v.submission_error,
            request_key: v.request_key,
            request_hash: v.request_hash,
            creator: actors.get(&v.creator_id)?,
            completed_by: actors.optional(v.completed_by_id.as_deref())?,
            commits: CommitContext {
                base: v.base_commit,
                head: v.head_commit,
            },
        })
    }

    pub async fn from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<sessions::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Session>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.creator_id.clone());
            if let Some(id) = &r.completed_by_id {
                ids.insert(id.clone());
            }
        }
        let actors = IdentityMap::load(ids, db).await?;
        rows.into_iter()
            .map(|r| Self::with_identities(r, &actors))
            .collect()
    }

    pub async fn from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: sessions::Model,
        db: &db::Database<C>,
    ) -> Result<Session> {
        Self::from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Session".into()))
    }
}
