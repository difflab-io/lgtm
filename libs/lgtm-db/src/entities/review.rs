//! SeaORM entity for the `reviews` table.

use crate::entities::common::{ReviewKind, ReviewStatus};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Local branch review or forge-backed pull request review record.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "reviews")]
pub struct Model {
    /// Internal UUID primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Sequential integer review number scoped to `git_dir` (`1, 2, 3...`).
    pub number: i64,
    /// Human-friendly display name (`R<number>-<branch>` or `PR<pr_number>-<branch>`).
    #[sea_orm(column_type = "Text")]
    pub name: String,
    /// Context discriminator (`Local` or `ForgeBacked`).
    pub kind: ReviewKind,
    /// Lifecycle status (`Open`, `Closed`, or `Merged`).
    pub status: ReviewStatus,
    /// Canonical path to the local `.git` directory.
    #[sea_orm(column_type = "Text")]
    pub git_dir: String,
    /// Checked-out source branch under review.
    #[sea_orm(column_type = "Text")]
    pub branch: String,
    /// Complete remote repository URL; required when `kind == ForgeBacked`, `None` when `Local`.
    #[sea_orm(column_type = "Text", nullable)]
    pub repository_url: Option<String>,
    /// Remote pull request number; required when `kind == ForgeBacked`, `None` when `Local`.
    pub pr_number: Option<i64>,
    /// Opaque remote pull request node identifier when synced with a forge.
    #[sea_orm(column_type = "Text", nullable)]
    pub forge_id: Option<String>,
    /// Base commit SHA of the reviewed diff range.
    #[sea_orm(column_type = "Text")]
    pub base_sha: String,
    /// Head commit SHA of the reviewed diff range.
    #[sea_orm(column_type = "Text")]
    pub head_sha: String,
    /// JSON object mapping reviewed file paths to the commit SHA at which each file was marked viewed.
    #[sea_orm(column_type = "Json")]
    pub reviewed_files: Json,
    /// Row creation timestamp in UTC.
    pub created_at: DateTimeUtc,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTimeUtc,
}

impl Model {
    // Public ------------------------------------------------------------------

    /// Decode `reviewed_files` into an ordered map of `file_path -> commit_sha`.
    pub fn get_reviewed_files_map(&self) -> BTreeMap<String, String> {
        serde_json::from_value(self.reviewed_files.clone()).unwrap_or_default()
    }
}

impl ActiveModelBehavior for ActiveModel {}
