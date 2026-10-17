//! Known canonical checkout or notes paths associated with a review context.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace_paths")]
/// Known canonical checkout or notes paths associated with a review context.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// UUID of the conversation that owns this record.
    pub context_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    /// Canonical absolute workspace checkout or notes path.
    pub path: String,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    /// Loaded relationship; not an additional stored column.
    pub context: BelongsTo<super::review_contexts::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
