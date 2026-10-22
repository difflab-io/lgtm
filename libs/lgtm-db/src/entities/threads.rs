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
    /// Immutable typed source identity and bounded relocation evidence; absent for provider-only coordinates.
    pub source_evidence: Option<Json>,
    /// Disposable provider location observation; rebuilt from GitHub during refresh.
    pub forge_observation: Option<Json>,
    /// Confirmed repository-relative path or absolute saved-file association. The original file_path never changes.
    pub current_path: Option<String>,
    /// Immutable absolute saved-file location for local notes. Git and forge anchors remain repository-relative.
    pub source_path: Option<String>,
    /// Confirmed file deletion or changed ephemeral working source. Absence is not proof of a valid line.
    pub orphan_reason: Option<crate::entities::common::OrphanReason>,
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

impl Model {
    /// Validate immutable evidence at insertion. Its fields are stored and replaced as one value.
    pub(crate) fn validate_source(&self) -> crate::error::Result<()> {
        use crate::error::invalid;
        let Some(evidence) = &self.source_evidence else {
            return Ok(());
        };
        let source = &evidence["source"];
        let hash = source["content_hash"].as_str().unwrap_or_default();
        let kind = source["kind"].as_str().unwrap_or_default();
        let object = |field: &str| {
            source[field].as_str().is_some_and(|value| {
                [40, 64].contains(&value.len())
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        };
        let valid_source = match kind {
            "commit" => object("sha"),
            "index_file" => object("blob_oid"),
            "working_file" | "local_file" => true,
            _ => false,
        };
        if !["file", "line"].contains(&self.anchor_kind.as_str())
            || !valid_source
            || !hash.strip_prefix("sha256:").is_some_and(|value| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            || ["selected_text", "context_before", "context_after"]
                .iter()
                .any(|field| !evidence[field].is_string())
            || self.source_path.is_some() && kind != "local_file"
            || self.anchor_kind == "file"
                && ["selected_text", "context_before", "context_after"]
                    .iter()
                    .any(|field| evidence[field] != "")
        {
            return Err(invalid("Incomplete or incompatible anchor evidence"));
        }
        Ok(())
    }

    pub(crate) fn validate_observation(&self) -> crate::error::Result<()> {
        use crate::error::invalid;
        let Some(observation) = &self.forge_observation else {
            return Ok(());
        };
        if ["base", "head"]
            .iter()
            .any(|field| observation[field].as_str().is_none_or(str::is_empty))
            || !observation["outdated"].is_boolean()
        {
            return Err(invalid("Incomplete forge observation"));
        }
        let current = &observation["current"];
        if current.is_null() {
            return Ok(());
        }
        if current["path"].as_str().is_none_or(str::is_empty)
            || match current["kind"].as_str() {
                Some("file") => false,
                Some("line") => {
                    current["start"].as_i64().is_none_or(|start| start < 1)
                        || current["end"]
                            .as_i64()
                            .is_none_or(|end| end < current["start"].as_i64().unwrap_or(1))
                        || ![Some("old"), Some("new")].contains(&current["side"].as_str())
                }
                _ => true,
            }
        {
            return Err(invalid("Incomplete forge location"));
        }
        Ok(())
    }
}

impl ActiveModelBehavior for ActiveModel {}
