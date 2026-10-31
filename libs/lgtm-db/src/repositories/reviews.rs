//! Repository for local branch reviews and forge-backed pull request reviews.

use crate::{
    entities::{
        common::{ReviewKind, ReviewStatus},
        review::{self, ActiveModel, Column, Entity, Model as Review},
    },
    error::{DbError, Result},
    repositories::{normalize_optional, require_non_empty},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

/// Filter criteria for listing reviews in a repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewFilter {
    /// Filter by local or forge-backed review kind.
    pub kind: Option<ReviewKind>,
    /// Filter by review lifecycle status (`Open`, `Closed`, `Merged`).
    pub status: Option<ReviewStatus>,
    /// Filter by exact source branch name.
    pub branch: Option<String>,
}

/// Parameters for creating a new local branch review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewLocalReview {
    /// Canonical path to the local `.git` directory.
    pub git_dir: String,
    /// Source branch name under review.
    pub branch: String,
    /// Base commit SHA of the diff range.
    pub base_sha: String,
    /// Head commit SHA of the diff range.
    pub head_sha: String,
}

/// Parameters for creating or updating a forge-backed pull request review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpsertForgeReview {
    /// Canonical path to the local `.git` directory.
    pub git_dir: String,
    /// Source branch name of the pull request.
    pub branch: String,
    /// Complete remote repository URL (`https://...`).
    pub repository_url: String,
    /// Pull request number on the remote forge.
    pub pr_number: i64,
    /// Optional opaque remote node identifier for the pull request.
    pub forge_id: Option<String>,
    /// Base commit SHA of the pull request diff.
    pub base_sha: String,
    /// Head commit SHA of the pull request diff.
    pub head_sha: String,
    /// Observed remote pull request status (`Open`, `Closed`, or `Merged`).
    pub status: ReviewStatus,
}

/// Persistence operations for `reviews` rows.
#[derive(Clone, Debug)]
pub struct ReviewsRepository<C> {
    pool: Arc<C>,
}

impl<C: ConnectionTrait> ReviewsRepository<C> {
    /// Construct a repository sharing the given connection or transaction handle.
    pub(crate) fn new(pool: Arc<C>) -> Self {
        Self { pool }
    }

    // Public ------------------------------------------------------------------

    /// Fetch a review by its internal UUID primary key.
    pub async fn get_review(&self, id: Uuid) -> Result<Option<Review>> {
        Ok(Entity::find_by_id(id).one(self.pool.as_ref()).await?)
    }

    /// Resolve a review within `git_dir` by number (`"42"`, `"R42"`), PR prefix (`"PR128"`), full name, or UUID.
    pub async fn get_review_by_selector(
        &self,
        git_dir: &str,
        selector: &str,
    ) -> Result<Option<Review>> {
        let git_dir = git_dir.trim();
        let selector = selector.trim();
        if git_dir.is_empty() || selector.is_empty() {
            return Ok(None);
        }

        if let Some(rest) = selector
            .strip_prefix("PR")
            .or_else(|| selector.strip_prefix("pr"))
            && let Ok(pr_number) = rest.parse::<i64>()
        {
            return Ok(Entity::find()
                .filter(Column::GitDir.eq(git_dir))
                .filter(Column::PrNumber.eq(pr_number))
                .order_by_desc(Column::Number)
                .one(self.pool.as_ref())
                .await?);
        }

        if let Some(rest) = selector
            .strip_prefix('R')
            .or_else(|| selector.strip_prefix('r'))
            && let Ok(number) = rest.parse::<i64>()
        {
            return Ok(Entity::find()
                .filter(Column::GitDir.eq(git_dir))
                .filter(Column::Number.eq(number))
                .one(self.pool.as_ref())
                .await?);
        }

        if let Ok(number) = selector.parse::<i64>() {
            if let Some(review) = Entity::find()
                .filter(Column::GitDir.eq(git_dir))
                .filter(Column::Number.eq(number))
                .one(self.pool.as_ref())
                .await?
            {
                return Ok(Some(review));
            }
            return Ok(Entity::find()
                .filter(Column::GitDir.eq(git_dir))
                .filter(Column::PrNumber.eq(number))
                .order_by_desc(Column::Number)
                .one(self.pool.as_ref())
                .await?);
        }

        if let Ok(id) = Uuid::parse_str(selector)
            && let Some(review) = Entity::find_by_id(id)
                .filter(Column::GitDir.eq(git_dir))
                .one(self.pool.as_ref())
                .await?
        {
            return Ok(Some(review));
        }

        Ok(Entity::find()
            .filter(Column::GitDir.eq(git_dir))
            .filter(Column::Name.eq(selector))
            .one(self.pool.as_ref())
            .await?)
    }

    /// Fetch the most recent review for `(git_dir, branch)`.
    pub async fn get_latest_review_by_branch(
        &self,
        git_dir: &str,
        branch: &str,
    ) -> Result<Option<Review>> {
        let git_dir = git_dir.trim();
        let branch = branch.trim();
        if git_dir.is_empty() || branch.is_empty() {
            return Ok(None);
        }
        Ok(Entity::find()
            .filter(Column::GitDir.eq(git_dir))
            .filter(Column::Branch.eq(branch))
            .order_by_desc(Column::Number)
            .one(self.pool.as_ref())
            .await?)
    }

    /// List reviews in `git_dir` matching `filter`, ordered from newest to oldest `number`.
    pub async fn get_reviews(&self, git_dir: &str, filter: ReviewFilter) -> Result<Vec<Review>> {
        let git_dir = git_dir.trim();
        let mut query = Entity::find().filter(Column::GitDir.eq(git_dir));
        if let Some(kind) = filter.kind {
            query = query.filter(Column::Kind.eq(kind));
        }
        if let Some(status) = filter.status {
            query = query.filter(Column::Status.eq(status));
        }
        if let Some(branch) = filter.branch {
            query = query.filter(Column::Branch.eq(branch.trim()));
        }
        Ok(query
            .order_by_desc(Column::Number)
            .all(self.pool.as_ref())
            .await?)
    }

    /// Create a new local branch review with the next sequential repo-scoped `number`.
    pub async fn create_local_review(&self, input: NewLocalReview) -> Result<Review> {
        let git_dir = require_non_empty("git_dir", &input.git_dir)?;
        let branch = require_non_empty("branch", &input.branch)?;
        let base_sha = require_non_empty("base_sha", &input.base_sha)?;
        let head_sha = require_non_empty("head_sha", &input.head_sha)?;

        let number = self.next_review_number(git_dir).await?;
        let name = format!("R{}-{}", number, Self::sanitize_branch(branch));
        let now = Utc::now();

        let active = review::ActiveModel {
            id: Set(Uuid::new_v4()),
            number: Set(number),
            name: Set(name),
            kind: Set(ReviewKind::Local),
            status: Set(ReviewStatus::Open),
            git_dir: Set(git_dir.to_owned()),
            branch: Set(branch.to_owned()),
            repository_url: Set(None),
            pr_number: Set(None),
            forge_id: Set(None),
            base_sha: Set(base_sha.to_owned()),
            head_sha: Set(head_sha.to_owned()),
            reviewed_files: Set(serde_json::json!({})),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Create or update a forge-backed pull request review keyed by `(git_dir, repository_url, pr_number)`.
    pub async fn upsert_forge_review(&self, input: UpsertForgeReview) -> Result<Review> {
        let git_dir = require_non_empty("git_dir", &input.git_dir)?;
        let branch = require_non_empty("branch", &input.branch)?;
        let repository_url = require_non_empty("repository_url", &input.repository_url)?;
        if !(repository_url.starts_with("https://") || repository_url.starts_with("http://")) {
            return Err(DbError::invalid_input(
                "repository_url must start with https:// or http://",
            ));
        }
        if input.pr_number <= 0 {
            return Err(DbError::invalid_input(
                "pr_number must be a positive integer",
            ));
        }
        let base_sha = require_non_empty("base_sha", &input.base_sha)?;
        let head_sha = require_non_empty("head_sha", &input.head_sha)?;
        let forge_id = normalize_optional(input.forge_id);
        let now = Utc::now();

        if let Some(existing) = Entity::find()
            .filter(Column::GitDir.eq(git_dir))
            .filter(Column::Kind.eq(ReviewKind::ForgeBacked))
            .filter(Column::RepositoryUrl.eq(repository_url))
            .filter(Column::PrNumber.eq(input.pr_number))
            .one(self.pool.as_ref())
            .await?
        {
            let name = self
                .format_unique_pr_name(
                    git_dir,
                    input.pr_number,
                    branch,
                    existing.number,
                    Some(existing.id),
                )
                .await?;
            let next_forge_id = forge_id.or(existing.forge_id.clone());
            let mut active: ActiveModel = existing.into();
            active.name = Set(name);
            active.branch = Set(branch.to_owned());
            active.forge_id = Set(next_forge_id);
            active.base_sha = Set(base_sha.to_owned());
            active.head_sha = Set(head_sha.to_owned());
            active.status = Set(input.status);
            active.updated_at = Set(now);
            return active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err);
        }

        let number = self.next_review_number(git_dir).await?;
        let name = self
            .format_unique_pr_name(git_dir, input.pr_number, branch, number, None)
            .await?;

        let active = review::ActiveModel {
            id: Set(Uuid::new_v4()),
            number: Set(number),
            name: Set(name),
            kind: Set(ReviewKind::ForgeBacked),
            status: Set(input.status),
            git_dir: Set(git_dir.to_owned()),
            branch: Set(branch.to_owned()),
            repository_url: Set(Some(repository_url.to_owned())),
            pr_number: Set(Some(input.pr_number)),
            forge_id: Set(forge_id),
            base_sha: Set(base_sha.to_owned()),
            head_sha: Set(head_sha.to_owned()),
            reviewed_files: Set(serde_json::json!({})),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Update the `base_sha` and `head_sha` commit range of an existing review.
    pub async fn update_review_commits(
        &self,
        id: Uuid,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<Review> {
        let base_sha = require_non_empty("base_sha", base_sha)?;
        let head_sha = require_non_empty("head_sha", head_sha)?;
        let existing = self.require_review(id).await?;
        let mut active: ActiveModel = existing.into();
        active.base_sha = Set(base_sha.to_owned());
        active.head_sha = Set(head_sha.to_owned());
        active.updated_at = Set(Utc::now());
        active
            .update(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Update the lifecycle `status` (`Open`, `Closed`, `Merged`) of an existing review.
    pub async fn update_review_status(&self, id: Uuid, status: ReviewStatus) -> Result<Review> {
        let existing = self.require_review(id).await?;
        let mut active: ActiveModel = existing.into();
        active.status = Set(status);
        active.updated_at = Set(Utc::now());
        active
            .update(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Record that `file_path` has been reviewed at `commit_sha` in `reviewed_files`.
    pub async fn mark_file_reviewed(
        &self,
        id: Uuid,
        file_path: &str,
        commit_sha: &str,
    ) -> Result<Review> {
        let file_path = require_non_empty("file_path", file_path)?;
        let commit_sha = require_non_empty("commit_sha", commit_sha)?;
        let existing = self.require_review(id).await?;
        let mut map = existing.get_reviewed_files_map();
        map.insert(file_path.to_owned(), commit_sha.to_owned());
        self.save_reviewed_files(existing, map).await
    }

    /// Remove `file_path` from the `reviewed_files` map of a review.
    pub async fn unmark_file_reviewed(&self, id: Uuid, file_path: &str) -> Result<Review> {
        let file_path = require_non_empty("file_path", file_path)?;
        let existing = self.require_review(id).await?;
        let mut map = existing.get_reviewed_files_map();
        map.remove(file_path);
        self.save_reviewed_files(existing, map).await
    }

    /// Replace the entire `reviewed_files` map of a review.
    pub async fn update_reviewed_files(
        &self,
        id: Uuid,
        reviewed_files: BTreeMap<String, String>,
    ) -> Result<Review> {
        for (path, sha) in &reviewed_files {
            require_non_empty("reviewed_files path", path)?;
            require_non_empty("reviewed_files commit_sha", sha)?;
        }
        let existing = self.require_review(id).await?;
        self.save_reviewed_files(existing, reviewed_files).await
    }

    // Private -----------------------------------------------------------------

    async fn require_review(&self, id: Uuid) -> Result<Review> {
        self.get_review(id)
            .await?
            .ok_or_else(|| DbError::not_found("review", id.to_string()))
    }

    async fn save_reviewed_files(
        &self,
        existing: Review,
        reviewed_files: BTreeMap<String, String>,
    ) -> Result<Review> {
        let json = serde_json::to_value(reviewed_files)
            .map_err(|err| DbError::invalid_input(err.to_string()))?;
        let mut active: ActiveModel = existing.into();
        active.reviewed_files = Set(json);
        active.updated_at = Set(Utc::now());
        active
            .update(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    async fn next_review_number(&self, git_dir: &str) -> Result<i64> {
        let latest = Entity::find()
            .filter(Column::GitDir.eq(git_dir))
            .order_by_desc(Column::Number)
            .one(self.pool.as_ref())
            .await?;
        Ok(latest.map_or(1, |row| row.number + 1))
    }

    async fn format_unique_pr_name(
        &self,
        git_dir: &str,
        pr_number: i64,
        branch: &str,
        review_number: i64,
        exclude_id: Option<Uuid>,
    ) -> Result<String> {
        let candidate = format!("PR{}-{}", pr_number, Self::sanitize_branch(branch));
        let collision = Entity::find()
            .filter(Column::GitDir.eq(git_dir))
            .filter(Column::Name.eq(&candidate))
            .one(self.pool.as_ref())
            .await?;
        if let Some(other) = collision
            && Some(other.id) != exclude_id
        {
            return Ok(format!("{candidate}-R{review_number}"));
        }
        Ok(candidate)
    }

    fn sanitize_branch(branch: &str) -> String {
        let mut sanitized = String::with_capacity(branch.len());
        let mut prev_dash = false;
        for ch in branch.trim().chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                sanitized.push(ch);
                prev_dash = false;
            } else if !prev_dash && !sanitized.is_empty() {
                sanitized.push('-');
                prev_dash = true;
            }
        }
        while sanitized.ends_with('-') {
            sanitized.pop();
        }
        if sanitized.is_empty() {
            "branch".to_string()
        } else {
            sanitized
        }
    }
}
