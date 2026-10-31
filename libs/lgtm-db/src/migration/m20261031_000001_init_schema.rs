//! Initial 4-table relational schema for identities, reviews, sessions, and comments.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. identities
        manager
            .create_table(
                Table::create()
                    .table("identities")
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(ColumnDef::new("name").text().not_null().unique_key())
                    .col(ColumnDef::new("kind").text().not_null())
                    .col(ColumnDef::new("email").text())
                    .col(ColumnDef::new("provider").text())
                    .col(ColumnDef::new("model").text())
                    .col(ColumnDef::new("forge_login").text())
                    .col(
                        ColumnDef::new("created_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new("updated_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .check(
                        Condition::any()
                            .add(
                                Condition::all()
                                    .add(Expr::col("kind").eq("Human"))
                                    .add(Expr::col("email").is_not_null())
                                    .add(Expr::col("email").ne(""))
                                    .add(Expr::col("provider").is_null())
                                    .add(Expr::col("model").is_null()),
                            )
                            .add(
                                Condition::all()
                                    .add(Expr::col("kind").eq("Agent"))
                                    .add(Expr::col("email").is_null())
                                    .add(Expr::col("provider").is_not_null())
                                    .add(Expr::col("provider").ne(""))
                                    .add(Expr::col("model").is_not_null())
                                    .add(Expr::col("model").ne("")),
                            ),
                    )
                    .to_owned(),
            )
            .await?;

        // 2. reviews
        manager
            .create_table(
                Table::create()
                    .table("reviews")
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(ColumnDef::new("number").big_integer().not_null())
                    .col(ColumnDef::new("name").text().not_null())
                    .col(ColumnDef::new("kind").text().not_null())
                    .col(ColumnDef::new("status").text().not_null())
                    .col(ColumnDef::new("git_dir").text().not_null())
                    .col(ColumnDef::new("branch").text().not_null())
                    .col(ColumnDef::new("repository_url").text())
                    .col(ColumnDef::new("pr_number").big_integer())
                    .col(ColumnDef::new("forge_id").text())
                    .col(ColumnDef::new("base_sha").text().not_null())
                    .col(ColumnDef::new("head_sha").text().not_null())
                    .col(ColumnDef::new("reviewed_files").json().not_null())
                    .col(
                        ColumnDef::new("created_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new("updated_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .check(
                        Condition::all()
                            .add(Expr::col("status").is_in(["Open", "Closed", "Merged"]))
                            .add(
                                Condition::any()
                                    .add(
                                        Condition::all()
                                            .add(Expr::col("kind").eq("Local"))
                                            .add(Expr::col("repository_url").is_null())
                                            .add(Expr::col("pr_number").is_null()),
                                    )
                                    .add(
                                        Condition::all()
                                            .add(Expr::col("kind").eq("ForgeBacked"))
                                            .add(Expr::col("repository_url").is_not_null())
                                            .add(Expr::col("repository_url").ne(""))
                                            .add(Expr::col("pr_number").is_not_null())
                                            .add(Expr::col("pr_number").gt(0)),
                                    ),
                            ),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_reviews_git_dir_number")
                    .table("reviews")
                    .unique()
                    .col("git_dir")
                    .col("number")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_reviews_git_dir_name")
                    .table("reviews")
                    .unique()
                    .col("git_dir")
                    .col("name")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_reviews_forge_pr")
                    .table("reviews")
                    .unique()
                    .col("git_dir")
                    .col("repository_url")
                    .col("pr_number")
                    .and_where(Expr::col("kind").eq("ForgeBacked"))
                    .to_owned(),
            )
            .await?;

        // 3. sessions
        manager
            .create_table(
                Table::create()
                    .table("sessions")
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(ColumnDef::new("review_id").uuid().not_null())
                    .col(ColumnDef::new("author_id").uuid().not_null())
                    .col(ColumnDef::new("number").big_integer().not_null())
                    .col(ColumnDef::new("state").text().not_null())
                    .col(ColumnDef::new("outcome").text())
                    .col(ColumnDef::new("body").text())
                    .col(ColumnDef::new("forge_id").text())
                    .col(ColumnDef::new("published_at").timestamp_with_time_zone())
                    .col(
                        ColumnDef::new("created_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new("updated_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_sessions_review")
                            .from("sessions", "review_id")
                            .to("reviews", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_sessions_author")
                            .from("sessions", "author_id")
                            .to("identities", "id")
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .check(
                        Condition::any()
                            .add(
                                Condition::all()
                                    .add(Expr::col("state").eq("Draft"))
                                    .add(Expr::col("outcome").is_null())
                                    .add(Expr::col("published_at").is_null()),
                            )
                            .add(
                                Condition::all()
                                    .add(Expr::col("state").eq("Published"))
                                    .add(
                                        Expr::col("outcome")
                                            .is_in(["Comment", "Reject", "Approve", "Request"]),
                                    )
                                    .add(Expr::col("published_at").is_not_null()),
                            ),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_sessions_review_number")
                    .table("sessions")
                    .unique()
                    .col("review_id")
                    .col("number")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_sessions_one_draft_per_author")
                    .table("sessions")
                    .unique()
                    .col("review_id")
                    .col("author_id")
                    .and_where(Expr::col("state").eq("Draft"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_sessions_review_forge_id")
                    .table("sessions")
                    .unique()
                    .col("review_id")
                    .col("forge_id")
                    .and_where(Expr::col("forge_id").is_not_null())
                    .to_owned(),
            )
            .await?;

        // 4. comments
        manager
            .create_table(
                Table::create()
                    .table("comments")
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new("number")
                            .big_integer()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new("review_id").uuid().not_null())
                    .col(ColumnDef::new("session_id").uuid().not_null())
                    .col(ColumnDef::new("author_id").uuid().not_null())
                    .col(ColumnDef::new("reply_to_id").uuid())
                    .col(ColumnDef::new("file_path").text())
                    .col(ColumnDef::new("start_line").integer())
                    .col(ColumnDef::new("line").integer())
                    .col(ColumnDef::new("commit_sha").text())
                    .col(ColumnDef::new("original_start_line").integer())
                    .col(ColumnDef::new("original_line").integer())
                    .col(ColumnDef::new("original_commit_sha").text())
                    .col(ColumnDef::new("body").text().not_null())
                    .col(
                        ColumnDef::new("resolved")
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new("outdated")
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new("forge_id").text())
                    .col(
                        ColumnDef::new("created_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new("updated_at")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_review")
                            .from("comments", "review_id")
                            .to("reviews", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_session")
                            .from("comments", "session_id")
                            .to("sessions", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_author")
                            .from("comments", "author_id")
                            .to("identities", "id")
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_reply_to")
                            .from("comments", "reply_to_id")
                            .to("comments", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_comments_review_forge_id")
                    .table("comments")
                    .unique()
                    .col("review_id")
                    .col("forge_id")
                    .and_where(Expr::col("forge_id").is_not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_comments_review_session")
                    .table("comments")
                    .col("review_id")
                    .col("session_id")
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("comments").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("sessions").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("reviews").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("identities").to_owned())
            .await
    }
}
