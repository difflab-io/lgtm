//! Notes retain their original identity when copied into a review.
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table("note_promotions")
                .col(ColumnDef::new("source_comment_id").uuid().not_null())
                .col(ColumnDef::new("target_session_id").uuid().not_null())
                .col(ColumnDef::new("target_comment_id").uuid().unique_key())
                .col(ColumnDef::new("request_key").text().not_null())
                .col(ColumnDef::new("payload_hash").text().not_null())
                .primary_key(
                    Index::create()
                        .col("source_comment_id")
                        .col("target_session_id"),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from("note_promotions", "target_session_id")
                        .to("sessions", "id"),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from("note_promotions", "target_comment_id")
                        .to("comments", "id")
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await
    }
    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table("note_promotions").to_owned())
            .await
    }
}
