//! A root comment and flat replies sharing one immutable original anchor and current resolution state.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "threads")]
/// A root comment and flat replies sharing one immutable original anchor and current resolution state.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Internal UUID primary key.
    pub id: uuid::Uuid,
    /// UUID of the conversation that owns this record.
    pub context_id: uuid::Uuid,
    /// UUID of the session that created this record; absent for externally imported records.
    pub origin_session_id: Option<uuid::Uuid>,
    /// UUID of the logical human or agent author, independent of the posting account.
    pub author_id: uuid::Uuid,
    /// Original anchor shape: general, file or line. Anchors are immutable after insertion.
    pub anchor_kind: String,
    /// Original repository-relative anchor path; absent for general comments.
    pub file_path: Option<String>,
    /// Original diff side, old or new, for a line anchor.
    pub side: Option<String>,
    /// Original commit SHA to which a line anchor belongs.
    pub reviewed_commit: Option<String>,
    /// Original inclusive starting line, using one-based indexing.
    pub start_line: Option<i64>,
    /// Original inclusive ending line, using one-based indexing.
    pub end_line: Option<i64>,
    /// Current lifecycle discriminator for this record.
    pub state: String,
    /// UUID of the last local resolver; absent when open or resolution was observed remotely.
    pub resolved_by_id: Option<uuid::Uuid>,
    /// UTC time of the last known local resolution.
    pub resolved_at: Option<String>,
    /// UUID of the identity responsible for the last local thread state change, if known.
    pub updated_by_id: Option<uuid::Uuid>,
    /// UTC time of the latest content or state update.
    pub updated_at: String,
    /// Opaque remote discussion/thread ID, scoped by context; NULL until linked.
    pub forge_thread_id: Option<String>,
    /// Whether the original line anchor refers to a commit different from the current reviewed commit.
    pub outdated: bool,

    #[sea_orm(belongs_to, relation_enum = "Author", from = "author_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub author: BelongsTo<super::identities::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "ResolvedBy",
        from = "resolved_by_id",
        to = "id"
    )]
    /// Loaded relationship; not an additional stored column.
    pub resolved_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(
        belongs_to,
        relation_enum = "UpdatedBy",
        from = "updated_by_id",
        to = "id"
    )]
    /// Loaded relationship; not an additional stored column.
    pub updated_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub context: BelongsTo<super::review_contexts::Entity>,

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
