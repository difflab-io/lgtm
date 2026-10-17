//! Repository natural keys shared by branch/PR contexts and workspace checkouts. A key is HOST/NAMESPACE/PROJECT or local:ABSOLUTE_GIT_COMMON_DIR, not a generated ID or URL.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "repositories")]
/// Repository natural keys shared by branch/PR contexts and workspace checkouts. A key is HOST/NAMESPACE/PROJECT or local:ABSOLUTE_GIT_COMMON_DIR, not a generated ID or URL.
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Canonical repository identity: HOST/NAMESPACE/PROJECT or local:ABSOLUTE_GIT_COMMON_DIR. This natural key is not a remote forge object ID.
    pub key: String,
}

impl ActiveModelBehavior for ActiveModel {}
