//! Save accepted local mutation results atomically with their domain writes.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table("mutation_receipts")
                .col(ColumnDef::new("context_id").uuid().not_null())
                .col(ColumnDef::new("actor_id").uuid().not_null())
                .col(ColumnDef::new("request_key").text().not_null())
                .col(ColumnDef::new("method").text().not_null())
                .col(ColumnDef::new("request_hash").text().not_null())
                .col(ColumnDef::new("result_json").text().not_null())
                .col(ColumnDef::new("version").big_integer().not_null())
                .col(ColumnDef::new("object_id").uuid())
                .col(
                    ColumnDef::new("pending")
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .primary_key(
                    Index::create()
                        .col("context_id")
                        .col("actor_id")
                        .col("request_key"),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from_tbl("mutation_receipts")
                        .from_col("context_id")
                        .to_tbl("review_contexts")
                        .to_col("id"),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from_tbl("mutation_receipts")
                        .from_col("actor_id")
                        .to_tbl("identities")
                        .to_col("id"),
                )
                .check(Expr::col("request_key").ne(""))
                .check(Expr::col("method").ne(""))
                .check(Expr::col("request_hash").ne(""))
                .check(Expr::col("version").gt(0))
                .check(
                    Expr::col("pending")
                        .eq(false)
                        .or(Expr::col("object_id").is_not_null()),
                )
                .to_owned(),
        )
        .await?;
        m.create_index(
            Index::create()
                .name("one_pending_object_mutation")
                .table("mutation_receipts")
                .col("context_id")
                .col("object_id")
                .unique()
                .and_where(Expr::col("pending").eq(true))
                .to_owned(),
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table("mutation_receipts").to_owned())
            .await
    }
}
