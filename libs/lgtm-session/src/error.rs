//! Error types returned by `lgtm-session` operations.

use lgtm_project::LgtmProjectError;

/// Result alias for `lgtm-session` operations.
pub type Result<T> = std::result::Result<T, LgtmSessionError>;

/// Errors produced by `lgtm-session` persistence and project orchestration.
#[derive(Debug, thiserror::Error)]
pub enum LgtmSessionError {
    /// Underlying project or worktree error.
    #[error("{0}")]
    Project(LgtmProjectError),

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

impl From<LgtmProjectError> for LgtmSessionError {
    fn from(value: LgtmProjectError) -> Self {
        match value {
            LgtmProjectError::NotFound(msg) => Self::NotFound(msg),
            other => Self::Project(other),
        }
    }
}
