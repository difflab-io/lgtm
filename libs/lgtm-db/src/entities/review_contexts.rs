use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "review_contexts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub kind: String,
    pub notes_path: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub host: Option<String>,
    pub number: Option<i64>,
    pub revision: i64,
    pub last_refreshed_at: Option<String>,
    pub refreshed_head: Option<String>,
    pub refreshed_base: Option<String>,
    pub refresh_error: Option<String>,
    pub refresh_status: String,
}

impl ActiveModelBehavior for ActiveModel {}
