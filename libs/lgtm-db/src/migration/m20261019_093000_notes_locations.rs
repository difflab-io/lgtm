//! Preserve saved-file origins while notes and directory review locations move.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.alter_table(
            Table::alter()
                .table("threads")
                .add_column(
                    ColumnDef::new("source_path")
                        .text()
                        .extra("CHECK (source_path IS NULL OR (source_kind IN ('unavailable', 'local_file') AND source_path LIKE '/%'))"),
                )
                .to_owned(),
        )
        .await?;
        m.alter_table(Table::alter().table("review_contexts")
            .add_column(ColumnDef::new("current_notes_path").text()
                .extra("CHECK (current_notes_path IS NULL OR (notes_path IS NOT NULL AND current_notes_path LIKE '/%'))"))
            .to_owned()).await?;
        m.create_index(
            Index::create()
                .name("idx_review_contexts_current_notes_path")
                .table("review_contexts")
                .col("current_notes_path")
                .unique()
                .to_owned(),
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_index(
            Index::drop()
                .name("idx_review_contexts_current_notes_path")
                .table("review_contexts")
                .to_owned(),
        )
        .await?;
        m.alter_table(
            Table::alter()
                .table("review_contexts")
                .drop_column("current_notes_path")
                .to_owned(),
        )
        .await?;
        m.alter_table(
            Table::alter()
                .table("threads")
                .drop_column("source_path")
                .to_owned(),
        )
        .await
    }
}
