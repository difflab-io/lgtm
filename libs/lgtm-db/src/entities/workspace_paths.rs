use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace_paths")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub context_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub path: String,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    pub context: BelongsTo<super::review_contexts::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
