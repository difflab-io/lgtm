//! Ordered SeaORM migrations embedded inside `lgtm-db`.

mod m20261031_000001_init_schema;

use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;

/// Embedded migration registry for `lgtm-db`.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261031_000001_init_schema::Migration)]
    }
}

/// Apply pending migrations atomically inside a transaction.
pub async fn apply<M: MigratorTrait>(db: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    let tx = db.begin().await?;
    M::up(&tx, None).await?;
    tx.commit().await
}
