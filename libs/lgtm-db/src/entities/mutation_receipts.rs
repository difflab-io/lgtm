//! Request acknowledgments and unresolved writes, scoped by actor, context, and retry key.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mutation_receipts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub context_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub actor_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_key: String,
    /// Method and normalized payload bind the key to one intended action.
    pub method: String,
    pub request_hash: String,
    /// Local accepted response, PR cache reference, or compact remote acknowledgment. No PR body history.
    pub result_json: String,
    /// Scope revision committed with the accepted result. Retries retain it.
    pub version: i64,
    /// Present only for a remote object intent. At most one unresolved intent may own it.
    pub object_id: Option<uuid::Uuid>,
    pub pending: bool,
}

impl ActiveModelBehavior for ActiveModel {}
