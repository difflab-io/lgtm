use super::super::database::models::comments;
use super::super::error::{Result, invalid};
use super::{Actor, PostingStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: uuid::Uuid,
    pub context_id: uuid::Uuid,
    pub thread_id: uuid::Uuid,
    pub origin_session_id: Option<uuid::Uuid>,
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
    pub posting: PostingStatus,
    pub posting_request_key: Option<String>,
    pub payload_hash: Option<String>,
    pub remote_presence: String,
}

impl From<&Comment> for comments::Model {
    fn from(v: &Comment) -> Self {
        Self {
            id: v.id,
            context_id: v.context_id,
            thread_id: v.thread_id,
            origin_session_id: v.origin_session_id,
            author_id: v.author.id,
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
            posting_state: v.posting.state(),
            posting_error: v.posting.error().map(str::to_owned),
            posting_request_key: v.posting_request_key.clone(),
            payload_hash: v.payload_hash.clone(),
            remote_presence: v.remote_presence.clone(),
        }
    }
}

impl Comment {
    pub fn from_db_model(v: comments::Model, author: Actor) -> Result<Self> {
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
            posting: PostingStatus::from_db(v.posting_state, v.posting_error)?,
            posting_request_key: v.posting_request_key,
            payload_hash: v.payload_hash,
            remote_presence: v.remote_presence,
            author,
        })
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
