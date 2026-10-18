//! A root or reply in a context-owned thread. Local attribution is retained independently of remote IDs and posting state.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "comments")]
/// A root or reply in a context-owned thread. Local attribution is retained independently of remote IDs and posting state.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Internal UUID primary key.
    pub id: uuid::Uuid,
    /// UUID of the conversation that owns this record.
    pub context_id: uuid::Uuid,
    /// UUID of the root/reply thread containing this comment.
    pub thread_id: uuid::Uuid,
    /// UUID of the session that created this record; absent for externally imported records.
    pub origin_session_id: Option<uuid::Uuid>,
    /// UUID of the logical human or agent author, independent of the posting account.
    pub author_id: uuid::Uuid,
    /// User-authored comment text, or optional session completion summary.
    pub body: String,
    /// Stable zero-based reply order within a thread; zero is the root comment.
    pub position: i64,
    /// Draft until session completion; published records belong to the context conversation.
    pub visibility: String,
    /// UTC creation timestamp; immutable after insertion.
    pub created_at: String,
    /// UTC time of the latest content or state update.
    pub updated_at: String,
    /// Opaque remote comment ID, interpreted together with forge_object_kind and context.
    pub forge_comment_id: Option<String>,
    /// Remote object namespace: review_comment, conversation_comment or review.
    pub forge_object_kind: Option<String>,
    /// Opaque remote review object ID when known; scoped by the owning context and forge.
    pub forge_review_id: Option<String>,
    /// Latest observed or prepared remote body, including the exact agent attribution footer.
    pub forge_body: Option<String>,
    /// Latest provider-supplied remote modification timestamp.
    pub forge_updated_at: Option<String>,
    /// Opaque authenticated forge account that actually posted the comment.
    pub posting_account_id: Option<String>,
    /// Typed posting lifecycle; an uncertain result requires reconciliation rather than blind retry.
    pub posting_state: crate::entities::common::PostingState,
    /// Required error details only for an uncertain posting state; NULL after confirmation.
    pub posting_error: Option<String>,
    /// Idempotency/reconciliation key of the attempted remote write.
    pub posting_request_key: Option<String>,
    /// Hash of the exact rendered outbound payload, used to correlate receipts and retries.
    pub payload_hash: Option<String>,
    /// Latest complete observation: local, present or missing. Partial fetches never mark records missing.
    pub remote_presence: String,

    #[sea_orm(belongs_to, relation_enum = "Author", from = "author_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub author: BelongsTo<super::identities::Entity>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub context: BelongsTo<super::review_contexts::Entity>,

    #[sea_orm(belongs_to, relation_enum = "Thread", from = "thread_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub thread: BelongsTo<super::threads::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "OriginSession",
        from = "origin_session_id",
        to = "id"
    )]
    /// Loaded relationship; not an additional stored column.
    pub origin_session: BelongsTo<Option<super::sessions::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
