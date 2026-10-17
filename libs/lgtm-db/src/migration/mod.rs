use sea_orm_migration::prelude::*;
mod m20261015_220400_review;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261015_220400_review::Migration)]
    }
}

/// Apply the framework's ordered migrations and history in a single transaction.
/// SQLite needs this wrapper; PostgreSQL's migrator nests a savepoint.
pub async fn apply<M: MigratorTrait>(db: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm::TransactionTrait;
    let tx = db.begin().await?;
    M::up(&tx, None).await?;
    tx.commit().await
}

#[cfg(test)]
mod tests;
