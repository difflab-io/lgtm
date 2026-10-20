//! Preserve local draft ownership independently of attribution and retain observed PR lifecycle.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.alter_table(
            Table::alter()
                .table("comments")
                .add_column(
                    ColumnDef::new("deleted")
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await?;
        m.alter_table(
            Table::alter()
                .table("comments")
                .add_column(
                    ColumnDef::new("owner_id")
                        .uuid()
                        .extra("REFERENCES identities(id)"),
                )
                .to_owned(),
        )
        .await?;
        m.get_connection()
            .execute_unprepared(
                "UPDATE comments SET owner_id = author_id WHERE origin_session_id IS NOT NULL",
            )
            .await?;
        m.alter_table(Table::alter().table("review_contexts")
            .add_column(ColumnDef::new("forge_state").text().extra("CHECK (forge_state IS NULL OR (kind = 'forge_backed' AND forge_state IN ('open','closed','merged')))"))
            .to_owned()).await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.alter_table(
            Table::alter()
                .table("comments")
                .drop_column("deleted")
                .to_owned(),
        )
        .await?;
        m.alter_table(
            Table::alter()
                .table("review_contexts")
                .drop_column("forge_state")
                .to_owned(),
        )
        .await?;
        m.alter_table(
            Table::alter()
                .table("comments")
                .drop_column("owner_id")
                .to_owned(),
        )
        .await
    }
}
