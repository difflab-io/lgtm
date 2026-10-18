//! Store only the selected excerpt needed to relocate non-Git note anchors.
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.alter_table(
            Table::alter()
                .table("threads")
                .add_column(ColumnDef::new("selected_text").text())
                .to_owned(),
        )
        .await
    }
    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.alter_table(
            Table::alter()
                .table("threads")
                .drop_column("selected_text")
                .to_owned(),
        )
        .await
    }
}
