use super::super::database::models::comments;
use super::super::{
    database as db,
    error::{Error, Result, invalid},
};
use super::Actor;
use super::identities::IdentityMap;
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub context_id: String,
    pub thread_id: String,
    pub origin_session_id: Option<String>,
    pub author: Actor,
    pub body: String,
    pub position: i64,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
    pub forge_comment_id: Option<String>,
    pub forge_object_kind: Option<String>,
    pub forge_review_id: Option<String>,
    pub forge_body: Option<String>,
    pub forge_updated_at: Option<String>,
    pub posting_account_id: Option<String>,
    pub posting_state: String,
    pub posting_error: Option<String>,
    pub posting_request_key: Option<String>,
    pub payload_hash: Option<String>,
    pub remote_presence: String,
}

impl From<&Comment> for comments::Model {
    fn from(v: &Comment) -> Self {
        Self {
            id: v.id.clone(),
            context_id: v.context_id.clone(),
            thread_id: v.thread_id.clone(),
            origin_session_id: v.origin_session_id.clone(),
            author_id: v.author.id.clone(),
            body: v.body.clone(),
            position: v.position,
            visibility: v.visibility.clone(),
            created_at: v.created_at.clone(),
            updated_at: v.updated_at.clone(),
            forge_comment_id: v.forge_comment_id.clone(),
            forge_object_kind: v.forge_object_kind.clone(),
            forge_review_id: v.forge_review_id.clone(),
            forge_body: v.forge_body.clone(),
            forge_updated_at: v.forge_updated_at.clone(),
            posting_account_id: v.posting_account_id.clone(),
            posting_state: v.posting_state.clone(),
            posting_error: v.posting_error.clone(),
            posting_request_key: v.posting_request_key.clone(),
            payload_hash: v.payload_hash.clone(),
            remote_presence: v.remote_presence.clone(),
        }
    }
}

impl Comment {
    fn with_identities(v: comments::Model, actors: &IdentityMap) -> Result<Self> {
        Ok(Comment {
            id: v.id,
            context_id: v.context_id,
            thread_id: v.thread_id,
            origin_session_id: v.origin_session_id,
            body: v.body,
            position: v.position,
            visibility: v.visibility,
            created_at: v.created_at,
            updated_at: v.updated_at,
            forge_comment_id: v.forge_comment_id,
            forge_object_kind: v.forge_object_kind,
            forge_review_id: v.forge_review_id,
            forge_body: v.forge_body,
            forge_updated_at: v.forge_updated_at,
            posting_account_id: v.posting_account_id,
            posting_state: v.posting_state,
            posting_error: v.posting_error,
            posting_request_key: v.posting_request_key,
            payload_hash: v.payload_hash,
            remote_presence: v.remote_presence,
            author: actors.get(&v.author_id)?,
        })
    }

    pub async fn from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<comments::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Comment>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.author_id.clone());
        }
        let actors = IdentityMap::load(ids, db).await?;
        rows.into_iter()
            .map(|r| Self::with_identities(r, &actors))
            .collect()
    }

    pub async fn from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: comments::Model,
        db: &db::Database<C>,
    ) -> Result<Comment> {
        Self::from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Comment".into()))
    }
}

impl Comment {
    pub fn validate_body(body: &str) -> Result<()> {
        if body.trim().is_empty() {
            Err(invalid("Comment body cannot be empty"))
        } else {
            Ok(())
        }
    }
}
