//! Error types returned by `lgtm-terminal` operations.

use thiserror::Error;

/// Result alias for `lgtm-terminal` operations.
pub type Result<T> = std::result::Result<T, LgtmTerminalError>;

/// Structured failures raised while talking to `herdr-core` (`hc`) or managing terminal sessions.
#[derive(Debug, Error)]
pub enum LgtmTerminalError {
    /// No bundled, staged, or system `hc` binary could be located.
    #[error("herdr-core (hc) binary was not found: {0}")]
    BinaryNotFound(String),

    /// The requested workspace directory does not exist or is not a directory.
    #[error("invalid terminal workspace directory: {0}")]
    InvalidWorkspace(String),

    /// The requested terminal session ID does not exist.
    #[error("terminal session not found: {0}")]
    NotFound(String),

    /// An RPC or transport failure occurred while communicating with `herdr-core`.
    #[error("herdr-core protocol error: {0}")]
    Protocol(String),

    /// Underlying process or I/O failure.
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
}
