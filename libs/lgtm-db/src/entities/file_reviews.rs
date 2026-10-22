//! Per-reviewer local file marks. Content identity invalidates a mark after source changes.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_reviews")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Local conversation; shared by staged and branch comparisons.
    pub context_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    /// Reviewer whose progress this row records.
    pub actor_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    /// Path within the selected comparison.
    pub path: String,
    /// Exact compared-content fingerprint; a changed fingerprint invalidates the mark.
    pub content_identity: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
