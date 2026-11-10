//! Error types returned by `lgtm-session` operations.

/// Result alias for `lgtm-session` operations.
pub type Result<T> = std::result::Result<T, LgtmSessionError>;

/// Errors produced by `lgtm-session` persistence and project orchestration.
#[derive(Debug, thiserror::Error)]
pub enum LgtmSessionError {
    /// Underlying project or worktree error.
    #[error("{0}")]
    Project(#[from] lgtm_project::LgtmProjectError),

    /// Target project or worktree was not found in the session.
    #[error("{0}")]
    NotFound(String),

    /// Session JSON serialization or deserialization error.
    #[error("session serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Direct filesystem I/O error.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
