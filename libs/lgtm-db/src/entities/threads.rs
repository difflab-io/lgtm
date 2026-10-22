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
    /// Selected non-Git lines only, used for unique relocation after content changes.
    pub selected_text: Option<String>,
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

    /// Immutable source discriminator. Unavailable means no saved-byte evidence was supplied.
    pub source_kind: crate::entities::common::SourceKind,
    /// Original stage-zero index blob ID, required only for index evidence.
    pub source_blob_oid: Option<String>,
    /// Immutable content commit, including file-level anchors with no line coordinates.
    pub source_commit: Option<String>,
    /// SHA-256 identity of the exact annotated bytes, including line endings.
    pub source_content_hash: Option<String>,
    /// Bounded original text immediately before the selected range.
    pub context_before: Option<String>,
    /// Bounded original text immediately after the selected range.
    pub context_after: Option<String>,
    /// Confirmed repository-relative path or absolute saved-file association. The original file_path never changes.
    pub current_path: Option<String>,
    /// Immutable absolute saved-file location for local notes. Git and forge anchors remain repository-relative.
    pub source_path: Option<String>,
    /// Confirmed file deletion or changed ephemeral working source. Absence is not proof of a valid line.
    pub orphan_reason: Option<crate::entities::common::OrphanReason>,
    /// Base identity of the last complete forge location observation.
    pub observed_base: Option<String>,
    /// Head identity of the last complete forge location observation.
    pub observed_head: Option<String>,
    /// Provider outdated flag, independent of resolution and local mapping.
    pub observed_outdated: Option<bool>,
    /// Provider current path, separate from the immutable original path.
    pub observed_path: Option<String>,
    /// Provider current diff side for a line observation.
    pub observed_side: Option<String>,
    /// Provider current inclusive first line. Never filled from historical lines.
    pub observed_start: Option<i64>,
    /// Provider current inclusive last line. Absent when no current line exists.
    pub observed_end: Option<i64>,

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
