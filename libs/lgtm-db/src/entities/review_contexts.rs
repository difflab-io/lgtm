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
    pub kind: crate::entities::common::ContextKind,
    /// Canonical absolute notes path; set only for local notes.
    pub notes_path: Option<String>,
    /// Confirmed current notes file or directory location. The original lookup key remains immutable.
    pub current_notes_path: Option<String>,
    /// Canonical absolute Git common directory; set only for local branch reviews.
    pub git_dir: Option<String>,
    /// Full refs/heads/ branch name; set only for local branch reviews.
    pub branch: Option<String>,
    /// Complete forge repository URL referencing repositories.repository_url; set only for forge-backed contexts.
    pub repository_url: Option<String>,
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
    /// Lifecycle from the last complete forge observation; absent for local contexts.
    pub forge_state: Option<crate::entities::common::ForgeState>,
}

impl ActiveModelBehavior for ActiveModel {}
