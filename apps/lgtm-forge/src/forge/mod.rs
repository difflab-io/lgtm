//! Provider-agnostic `RemoteForge` trait and remote forge implementations.

pub mod github;
pub mod models;

pub use github::GitHubForge;
pub use models::{
    ForgeCommentUpdate, ForgeDraftCommentInput, ForgeDraftCommentReceipt, ForgePullRequest,
    ForgePullRequestBundle, ForgeReviewComment, ForgeReviewReceipt, ForgeReviewSession,
};

use crate::{error::Result, models::ReviewOutcome};

/// Provider-agnostic interface for synchronizing pull/merge requests, pending reviews, comments, and viewed-file states with a remote forge.
#[async_trait::async_trait]
pub trait RemoteForge: Send + Sync {
    /// Pull the latest pull request metadata, review sessions, and comments for `branch_or_pr`.
    async fn get_pull_request_bundle(
        &self,
        repository_url: &str,
        branch_or_pr: &str,
    ) -> Result<ForgePullRequestBundle>;

    /// Create a pending review session on the remote forge for `pr_number` at `head_sha`.
    async fn create_pending_review(
        &self,
        repository_url: &str,
        pr_number: u64,
        head_sha: &str,
    ) -> Result<String>;

    /// Create a draft comment on a remote pull request (creating a pending review first if `pending_review_id` is `None`).
    async fn create_remote_draft_comment(
        &self,
        repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        comment: &ForgeDraftCommentInput,
    ) -> Result<ForgeDraftCommentReceipt>;

    /// Update an existing comment's body, anchor, or thread resolution state on the remote forge.
    async fn update_remote_comment(
        &self,
        repository_url: &str,
        forge_id: &str,
        update: &ForgeCommentUpdate,
    ) -> Result<()>;

    /// Delete a comment from the remote forge.
    async fn delete_remote_comment(&self, repository_url: &str, forge_id: &str) -> Result<()>;

    /// Mark or unmark `file_path` as viewed on the remote pull request.
    async fn set_remote_file_viewed(
        &self,
        repository_url: &str,
        pr_forge_id: Option<&str>,
        pr_number: u64,
        file_path: &str,
        viewed: bool,
    ) -> Result<()>;

    /// Submit a pending review session with `outcome` and optional summary `body` on the remote forge.
    async fn submit_pending_review(
        &self,
        repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        outcome: ReviewOutcome,
        body: Option<&str>,
    ) -> Result<ForgeReviewReceipt>;
}
