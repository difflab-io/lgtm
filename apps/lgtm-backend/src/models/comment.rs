use super::super::database::models::comments;
use super::super::error::{Result, invalid};
use super::{Actor, PostingStatus, Session, Thread};
use crate::utils::now;
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: uuid::Uuid,
    pub context_id: uuid::Uuid,
    pub thread_id: uuid::Uuid,
    pub origin_session_id: Option<uuid::Uuid>,
    pub author: Actor,
    /// Draft ownership is distinct from preserved source attribution.
    #[serde(default)]
    pub owner_id: Option<uuid::Uuid>,
    /// Original note identity for copied feedback; no ownership or deletion restriction.
    pub source_note_id: Option<uuid::Uuid>,
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
}

impl Comment {
    // Public ------------------------------------------------------------------

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
            owner_id: v.owner_id,
            source_note_id: v.source_note_id,
            author,
        })
    }

    /// Copy feedback to a new review while retaining its source author.
    pub fn promote(source: &Self, target: &Session, thread: &Thread, owner: &Actor) -> Self {
        let mut comment = Self::local(target, thread, &source.author, &source.body, 0);
        comment.owner_id = Some(owner.id);
        comment.source_note_id = Some(source.id);
        comment
    }

    pub fn validate_body(body: &str) -> Result<()> {
        if body.trim().is_empty() {
            Err(invalid("Comment body cannot be empty"))
        } else {
            Ok(())
        }
    }

    /// Construct a local draft comment attributed to its creating session and actor.
    pub fn local(s: &Session, t: &Thread, actor: &Actor, body: &str, position: i64) -> Self {
        Comment {
            id: uuid::Uuid::new_v4(),
            context_id: s.context_id,
            thread_id: t.id,
            origin_session_id: Some(s.id),
            author: actor.clone(),
            owner_id: Some(actor.id),
            source_note_id: None,
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
            posting: PostingStatus::Unposted,
            posting_request_key: None,
            payload_hash: None,
        }
    }

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value and include its concurrency token.
    pub(crate) fn into_versioned_proto(
        self,
        version: i64,
    ) -> std::result::Result<p::Comment, ConnectError> {
        let value = self;
        let token = crate::models::ExpectedObject::get_version(
            m::ObjectKind::Comment,
            version,
            value.id,
            &value,
        )
        .map_err(ConnectError::from)?;
        let mut value = m::Comment::into_proto(value);
        value.version = token;
        Ok(value)
    }

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::Comment {
        let v = self;
        p::Comment {
            id: v.id.to_string(),
            context_id: v.context_id.to_string(),
            thread_id: v.thread_id.to_string(),
            origin_session_id: v
                .origin_session_id
                .map(|x| x.to_string())
                .unwrap_or_default(),
            posting_status: v.posting.state().as_str().into(),
            owner_id: v.owner_id.map(|id| id.to_string()).unwrap_or_default(),
            source_note_id: v
                .source_note_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            author: Some(m::Actor::into_proto(v.author)).into(),
            body: v.body,
            position: v.position,
            visibility: v.visibility,
            created_at: v.created_at,
            updated_at: v.updated_at,
            forge_comment_id: v.forge_comment_id.unwrap_or_default(),

            ..Default::default()
        }
    }
}

impl From<&Comment> for comments::Model {
    fn from(v: &Comment) -> Self {
        Self {
            id: v.id,
            context_id: v.context_id,
            thread_id: v.thread_id,
            origin_session_id: v.origin_session_id,
            author_id: v.author.id,
            owner_id: v.owner_id,
            source_note_id: v.source_note_id,
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
        }
    }
}
