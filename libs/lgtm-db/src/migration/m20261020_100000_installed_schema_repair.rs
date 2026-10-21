//! Repair early installed variants whose migration names were recorded before their contracts stabilized.
//! Keep this additive: published migration names must never be reused for schema changes.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const SOURCE_CHECKS: &str = r#"CHECK ((source_kind = 'legacy' AND source_commit IS NULL AND source_blob_oid IS NULL AND source_content_hash IS NULL AND context_before IS NULL AND context_after IS NULL)  OR (anchor_kind IN ('line','file') AND selected_text IS NOT NULL AND source_content_hash IS NOT NULL AND source_content_hash LIKE 'sha256:%' AND length(source_content_hash) = 71  AND ((source_kind = 'commit' AND source_blob_oid IS NULL AND source_commit IS NOT NULL AND length(source_commit) IN (40,64) AND (anchor_kind = 'file' OR reviewed_commit = source_commit))  OR (source_kind = 'index_file' AND source_commit IS NULL AND source_blob_oid IS NOT NULL AND length(source_blob_oid) IN (40,64))  OR (source_kind IN ('working_file','local_file') AND source_commit IS NULL AND source_blob_oid IS NULL)))) CHECK (anchor_kind <> 'file' OR source_kind = 'legacy' OR (selected_text = '' AND context_before = '' AND context_after = '')) CHECK ((observed_outdated IS NULL AND observed_base IS NULL AND observed_head IS NULL AND observed_path IS NULL AND observed_side IS NULL AND observed_start IS NULL AND observed_end IS NULL AND observed_commit IS NULL) OR (observed_outdated IS NOT NULL AND observed_base IS NOT NULL AND observed_head IS NOT NULL AND ((observed_start IS NULL AND observed_end IS NULL AND observed_side IS NULL AND observed_commit IS NULL) OR (observed_path IS NOT NULL AND observed_start IS NOT NULL AND observed_end IS NOT NULL AND observed_start > 0 AND observed_end >= observed_start AND observed_side IS NOT NULL AND observed_side IN ('old','new') AND observed_commit IS NOT NULL AND length(observed_commit) > 0))))"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        let db = m.get_connection();
        if !m.has_column("mutation_receipts", "object_id").await? {
            m.alter_table(
                Table::alter()
                    .table("mutation_receipts")
                    .add_column(ColumnDef::new("object_id").uuid())
                    .to_owned(),
            )
            .await?;
        }
        if !m.has_column("mutation_receipts", "pending").await? {
            m.alter_table(
                Table::alter()
                    .table("mutation_receipts")
                    .add_column(
                        ColumnDef::new("pending")
                            .boolean()
                            .not_null()
                            .default(false)
                            .extra("CHECK (pending = false OR object_id IS NOT NULL)"),
                    )
                    .to_owned(),
            )
            .await?;
        }
        if !m
            .has_index("mutation_receipts", "one_pending_object_mutation")
            .await?
        {
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
            .await?;
        }
        if !m.has_column("comments", "deleted").await? {
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
        }
        if !m.has_column("threads", "source_commit").await? {
            // Replace only the changed evidence columns and their column checks. Parent tables
            // and their referenced keys stay in place, including SQLite with foreign keys on.
            db.execute_unprepared("CREATE TEMPORARY TABLE repair_thread_sources AS SELECT id, source_kind, source_path, source_blob_oid, source_content_hash, context_before, context_after, CASE WHEN source_kind = 'commit' THEN reviewed_commit ELSE NULL END AS source_commit FROM threads").await?;
            for column in ["source_path", "source_kind"] {
                m.alter_table(
                    Table::alter()
                        .table("threads")
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
            }
            db.execute_unprepared("UPDATE threads SET source_blob_oid = NULL, source_content_hash = NULL, context_before = NULL, context_after = NULL").await?;
            m.alter_table(
                Table::alter()
                    .table("threads")
                    .add_column(ColumnDef::new("source_commit").text())
                    .to_owned(),
            )
            .await?;
            m.alter_table(
                Table::alter()
                    .table("threads")
                    .add_column(
                        ColumnDef::new("source_kind")
                            .text()
                            .not_null()
                            .default("legacy")
                            .extra(SOURCE_CHECKS),
                    )
                    .to_owned(),
            )
            .await?;
            m.alter_table(Table::alter().table("threads")
                .add_column(ColumnDef::new("source_path").text().extra("CHECK (source_path IS NULL OR (source_kind IN ('legacy','local_file') AND source_path LIKE '/%'))")).to_owned()).await?;
            db.execute_unprepared("UPDATE threads SET (source_kind, source_path, source_blob_oid, source_content_hash, context_before, context_after, source_commit) = (SELECT source_kind, source_path, source_blob_oid, source_content_hash, context_before, context_after, source_commit FROM repair_thread_sources WHERE repair_thread_sources.id = threads.id)").await?;
            db.execute_unprepared("DROP TABLE repair_thread_sources")
                .await?;
        }
        // Both recorded variants have this column, but only the later one accepts hidden
        // notes sessions. Preserve every comparison while replacing its column check.
        db.execute_unprepared("CREATE TEMPORARY TABLE repair_session_comparisons AS SELECT id, comparison_kind FROM sessions").await?;
        m.alter_table(
            Table::alter()
                .table("sessions")
                .drop_column("comparison_kind")
                .to_owned(),
        )
        .await?;
        m.alter_table(Table::alter().table("sessions")
            .add_column(ColumnDef::new("comparison_kind").text().not_null().default("legacy")
                .extra("CHECK (comparison_kind IN ('legacy','notes','working','staged','branch','directory','pull_request'))")).to_owned()).await?;
        db.execute_unprepared("UPDATE sessions SET comparison_kind = (SELECT comparison_kind FROM repair_session_comparisons WHERE repair_session_comparisons.id = sessions.id)").await?;
        db.execute_unprepared("DROP TABLE repair_session_comparisons")
            .await?;
        if !m
            .has_index("review_contexts", "idx_review_contexts_current_notes_path")
            .await?
        {
            m.create_index(
                Index::create()
                    .name("idx_review_contexts_current_notes_path")
                    .table("review_contexts")
                    .col("current_notes_path")
                    .unique()
                    .to_owned(),
            )
            .await?;
        }
        Ok(())
    }

    async fn down(&self, _m: &SchemaManager) -> Result<(), DbErr> {
        // Repair is intentionally retained; reverting to a broken installed variant is unsafe.
        // Earlier migrations still own and remove these columns during a full rollback.
        Ok(())
    }
}
