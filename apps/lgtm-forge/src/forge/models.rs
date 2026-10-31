//! Provider-neutral remote forge import and export contracts.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::models::{ReviewOutcome, ReviewStatus, SessionState};

/// Complete remote pull request bundle returned by a `RemoteForge` implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgePullRequestBundle {
    pub pull_request: ForgePullRequest,
    pub sessions: Vec<ForgeReviewSession>,
    pub comments: Vec<ForgeReviewComment>,
}

/// Remote pull request metadata and viewed-file state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgePullRequest {
    pub pr_number: u64,
    pub forge_id: String,
    pub branch: String,
    pub base_sha: String,
    pub head_sha: String,
    pub status: ReviewStatus,
    pub reviewed_files: BTreeMap<String, String>,
}

/// Remote review session (pending or submitted) imported from a forge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeReviewSession {
    pub forge_id: String,
    pub author_name: String,
    pub author_email: String,
    pub author_login: String,
    pub state: SessionState,
    pub outcome: Option<ReviewOutcome>,
    pub body: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

/// Remote inline or thread reply comment imported from a forge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeReviewComment {
    pub forge_id: String,
    pub session_forge_id: String,
    pub reply_to_forge_id: Option<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_login: String,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub line: Option<u32>,
    pub commit_sha: Option<String>,
    pub original_start_line: Option<u32>,
    pub original_line: Option<u32>,
    pub original_commit_sha: Option<String>,
    pub body: String,
    pub resolved: bool,
    pub outdated: bool,
}

/// Payload for creating a remote draft comment on a pending review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeDraftCommentInput {
    pub body: String,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub line: Option<u32>,
    pub commit_sha: String,
    pub reply_to_forge_id: Option<String>,
}

/// Receipt returned when a remote draft comment is created on a pending review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeDraftCommentReceipt {
    pub pending_review_forge_id: String,
    pub comment_forge_id: String,
}

/// Payload for updating an existing remote comment's body, anchor, or thread resolution state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeCommentUpdate {
    pub body: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub line: Option<u32>,
    pub commit_sha: Option<String>,
    pub resolved: Option<bool>,
}

/// Receipt returned when a pending review session is submitted on a remote forge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeReviewReceipt {
    pub review_forge_id: String,
    pub published_at: DateTime<Utc>,
}
