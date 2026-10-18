//! A review attempt over a context and reviewed commits. At most one session is active per context; completion metadata is retained.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
/// A review attempt over a context and reviewed commits. At most one session is active per context; completion metadata is retained.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Internal UUID primary key.
    pub id: uuid::Uuid,
    /// UUID of the conversation that owns this record.
    pub context_id: uuid::Uuid,
    /// UUID of the identity that started the session.
    pub creator_id: uuid::Uuid,
    /// Current lifecycle discriminator for this record.
    pub state: super::super::types::SessionState,
    /// Base commit reviewed by this session; immutable after creation.
    pub base_commit: Option<String>,
    /// Head commit reviewed by this session; immutable after creation.
    pub head_commit: Option<String>,
    /// UTC creation timestamp; immutable after insertion.
    pub created_at: String,
    /// UTC completion time; required only for completed sessions.
    pub completed_at: Option<String>,
    /// UUID of the identity that completed the session; absent while active.
    pub completed_by_id: Option<uuid::Uuid>,
    /// Completion outcome: saved, ready-to-address, approve, reject or comment as allowed by context.
    pub outcome: Option<String>,
    /// User-authored comment text, or optional session completion summary.
    pub body: Option<String>,
    /// Opaque remote review object ID when known; scoped by the owning context and forge.
    pub forge_review_id: Option<String>,
    /// Current remote review submission result; local for non-forge sessions.
    pub submission_status: String,
    /// Submission failure or stub explanation, when applicable.
    pub submission_error: Option<String>,
    /// Caller-supplied completion idempotency key, unique within the context.
    pub request_key: Option<String>,
    /// Hash of completion input and actor used to validate retries.
    pub request_hash: Option<String>,

    #[sea_orm(belongs_to, relation_enum = "Creator", from = "creator_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub creator: BelongsTo<super::identities::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "CompletedBy",
        from = "completed_by_id",
        to = "id"
    )]
    /// Loaded relationship; not an additional stored column.
    pub completed_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub context: BelongsTo<super::review_contexts::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
