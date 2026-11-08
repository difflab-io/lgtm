//! Error types returned by `lgtm-rsutils` operations.

/// Result alias for `lgtm-rsutils` operations.
pub type Result<T> = std::result::Result<T, RsUtilsError>;

/// Errors produced by Git path resolution, worktree commands, atomic writes, and URL parsing.
#[derive(Debug, thiserror::Error)]
pub enum RsUtilsError {
    /// Invalid input path, branch name, or repository URL.
    #[error("{0}")]
    Invalid(String),

    /// Branch is already checked out in another Git worktree.
    #[error("{0}")]
    BranchAlreadyCheckedOut(String),

    /// Target worktree has modified or untracked files and requires `force = true` for removal.
    #[error("{0}")]
    DirtyWorktree(String),

    /// Underlying `git` CLI command failure.
    #[error("{0}")]
    Git(String),

    /// Filesystem I/O failure.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
