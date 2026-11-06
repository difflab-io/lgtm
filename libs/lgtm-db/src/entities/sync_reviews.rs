//! Accepted remote baseline and one durable pending upload per context.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sync_reviews")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub context_id: uuid::Uuid,
    pub version: i64,
    pub baseline: Option<Json>,
    pub pending: Option<Json>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
