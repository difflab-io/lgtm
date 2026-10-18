//! Forge repository URLs shared by PR/MR contexts.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "repositories")]
/// A forge repository identified by its complete HTTP(S) URL.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Complete canonical forge repository URL.
    pub repository_url: String,
}

impl ActiveModelBehavior for ActiveModel {}
