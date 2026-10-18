//! Logical human and agent identities referenced by sessions, threads and comments. Forge account bindings are independent of agent providers.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "identities")]
/// Logical human and agent identities referenced by sessions, threads and comments. Forge account bindings are independent of agent providers.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Internal UUID primary key.
    pub id: uuid::Uuid,
    /// Human-readable identity display name.
    pub name: String,
    /// Typed discriminator selecting the permitted record shape.
    pub kind: crate::entities::common::IdentityKind,
    /// Required human email; NULL for agents.
    pub email: Option<String>,
    /// Required agent model name; NULL for humans.
    pub model: Option<String>,
    /// Required agent model provider; NULL for humans. This is not a forge account identifier.
    pub provider: Option<String>,
    /// Scoped opaque forge account binding, encoded from host and account ID; empty for local identities.
    pub forge_account: String,
}

impl ActiveModelBehavior for ActiveModel {}
