//! Error types returned by `lgtm-project` operations.

/// Result alias for `lgtm-project` operations.
pub type Result<T> = std::result::Result<T, LgtmProjectError>;

/// Domain and filesystem errors produced by `lgtm-project`.
#[derive(Debug, thiserror::Error)]
pub enum LgtmProjectError {
    /// Branch name is empty or rejected by `git check-ref-format`.
    #[error("{0}")]
    InvalidBranch(String),

    /// Branch is already checked out in another Git worktree.
    #[error("{0}")]
    BranchAlreadyCheckedOut(String),

    /// Target managed worktree directory already exists on disk.
    #[error("{0}")]
    CheckoutPathExists(String),

    /// Project root is not a Git repository.
    #[error("{0}")]
    NotGitRepository(String),

    /// Managed worktree has modified or untracked files and requires `force = true` for removal.
    #[error("{0}")]
    DirtyWorktree(String),

    /// Invalid project or worktree path.
    #[error("{0}")]
    InvalidPath(String),

    /// Target project or worktree was not found.
    #[error("{0}")]
    NotFound(String),

    /// Underlying `git` CLI or filesystem error.
    #[error("{0}")]
    Git(String),
}
