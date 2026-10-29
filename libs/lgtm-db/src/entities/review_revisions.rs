//! Immutable source revisions retained by a review session.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "review_revisions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    /// Monotonic within the session; the greatest value is its selected revision.
    pub sequence: i64,
    pub repository_id: uuid::Uuid,
    /// Backend-local managed Git worktree, never a URL.
    pub root: String,
    pub base_commit: String,
    pub head_commit: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
