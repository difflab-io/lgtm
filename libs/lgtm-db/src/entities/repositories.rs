use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "repositories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
}

impl ActiveModelBehavior for ActiveModel {}
