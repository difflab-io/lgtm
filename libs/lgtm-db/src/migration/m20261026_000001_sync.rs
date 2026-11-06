//! POC local sync bookkeeping; no triggers or generic change-capture machinery.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("sync_account")
                    .col(ColumnDef::new("id").integer().primary_key())
                    .col(ColumnDef::new("owner_id").uuid().not_null())
                    .col(ColumnDef::new("cursor").big_integer().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table("sync_reviews")
                    .col(ColumnDef::new("context_id").uuid().primary_key())
                    .col(ColumnDef::new("version").big_integer().not_null())
                    .col(ColumnDef::new("baseline").json_binary())
                    .col(ColumnDef::new("pending").json_binary())
                    .to_owned(),
            )
            .await
    }
}
