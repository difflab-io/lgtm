//! SeaORM entity for the `identities` table.

use crate::entities::common::IdentityKind;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Human reviewer or AI agent identity record.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "identities")]
pub struct Model {
    /// Internal UUID primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Unique handle or display name (`--as <agent-name>` or Git user name).
    #[sea_orm(column_type = "Text", unique)]
    pub name: String,
    /// Discriminator selecting `Human` or `Agent` invariants.
    pub kind: IdentityKind,
    /// Required email address when `kind == Human`; `None` when `kind == Agent`.
    #[sea_orm(column_type = "Text", nullable)]
    pub email: Option<String>,
    /// Required model provider when `kind == Agent`; `None` when `kind == Human`.
    #[sea_orm(column_type = "Text", nullable)]
    pub provider: Option<String>,
    /// Required model identifier when `kind == Agent`; `None` when `kind == Human`.
    #[sea_orm(column_type = "Text", nullable)]
    pub model: Option<String>,
    /// Optional remote forge login (for example, GitHub username).
    #[sea_orm(column_type = "Text", nullable)]
    pub forge_login: Option<String>,
    /// Row creation timestamp in UTC.
    pub created_at: DateTimeUtc,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
