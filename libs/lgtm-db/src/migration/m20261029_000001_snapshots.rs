//! Add retained source revisions without discarding existing conversations or notes.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("review_revisions")
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(ColumnDef::new("session_id").uuid().not_null())
                    .col(ColumnDef::new("sequence").big_integer().not_null())
                    .col(ColumnDef::new("repository_id").uuid().not_null())
                    .col(ColumnDef::new("root").text().not_null())
                    .col(ColumnDef::new("base_commit").text().not_null())
                    .col(ColumnDef::new("head_commit").text().not_null())
                    .col(ColumnDef::new("created_at").text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from("review_revisions", "session_id")
                            .to("sessions", "id"),
                    )
                    .index(
                        Index::create()
                            .name("revision_sequence")
                            .col("session_id")
                            .col("sequence")
                            .unique(),
                    )
                    .check(Expr::col("sequence").gt(0))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("review_revisions").to_owned())
            .await
    }
}
