//! SeaORM entity for the `comments` table.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// General, single-line, range, or reply comment belonging to a review session.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "comments")]
pub struct Model {
    /// Internal UUID primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Global sequential comment number (`C1, C2, ...`).
    #[sea_orm(unique)]
    pub number: i64,
    /// Foreign key referencing `reviews.id`.
    pub review_id: Uuid,
    /// Foreign key referencing `sessions.id`.
    pub session_id: Uuid,
    /// Foreign key referencing `identities.id` for the comment author.
    pub author_id: Uuid,
    /// Optional foreign key referencing the root `comments.id` when this row is a reply.
    pub reply_to_id: Option<Uuid>,
    /// Relative repository file path (`None` for general review-level comments).
    #[sea_orm(column_type = "Text", nullable)]
    pub file_path: Option<String>,
    /// Optional starting line number for a multi-line range comment.
    pub start_line: Option<i32>,
    /// Single target line number or ending line number of a multi-line range.
    pub line: Option<i32>,
    /// Commit SHA against which `start_line` and `line` are currently anchored.
    #[sea_orm(column_type = "Text", nullable)]
    pub commit_sha: Option<String>,
    /// Initial starting line number recorded when the comment was created.
    pub original_start_line: Option<i32>,
    /// Initial ending or single line number recorded when the comment was created.
    pub original_line: Option<i32>,
    /// Initial commit SHA recorded when the comment was created.
    #[sea_orm(column_type = "Text", nullable)]
    pub original_commit_sha: Option<String>,
    /// Markdown body text of the comment.
    #[sea_orm(column_type = "Text")]
    pub body: String,
    /// Whether the conversation thread is resolved (meaningful on root comments).
    pub resolved: bool,
    /// Whether subsequent commits invalidated the comment's diff anchor.
    pub outdated: bool,
    /// Opaque remote comment identifier when synced with a forge.
    #[sea_orm(column_type = "Text", nullable)]
    pub forge_id: Option<String>,
    /// Row creation timestamp in UTC.
    pub created_at: DateTimeUtc,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
