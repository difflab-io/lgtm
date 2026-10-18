//! Persistent notes, branch or forge-review conversations. Sessions share the context-owned thread and comment history.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "review_contexts")]
/// Persistent notes, branch or forge-review conversations. Sessions share the context-owned thread and comment history.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Internal UUID primary key.
    pub id: uuid::Uuid,
    /// Typed discriminator selecting the permitted record shape.
    pub kind: super::super::types::ContextKind,
    /// Canonical absolute path identifying a notes context; NULL for branch/PR contexts.
    pub notes_path: Option<String>,
    /// Canonical repository key referencing repositories.key; NULL for path-only notes.
    pub repository: Option<String>,
    /// Full refs/heads/ branch name for local review; NULL for other context kinds.
    pub branch: Option<String>,
    /// Forge hostname for PR/MR contexts, including self-hosted instances.
    pub host: Option<String>,
    /// Repository/project-scoped PR or MR number, not a global forge object ID.
    pub number: Option<i64>,
    /// Monotonic context write revision used for locking and stale-refresh detection.
    pub revision: i64,
    /// UTC time of the most recent complete forge observation, if any.
    pub last_refreshed_at: Option<String>,
    /// Head commit observed by the most recent complete refresh.
    pub refreshed_head: Option<String>,
    /// Base commit observed by the most recent complete refresh, if supplied.
    pub refreshed_base: Option<String>,
    /// Latest refresh failure details; cleared after a successful observation.
    pub refresh_error: Option<String>,
    /// Latest refresh result: never, stub, live or failed.
    pub refresh_status: String,
}

impl ActiveModelBehavior for ActiveModel {}
