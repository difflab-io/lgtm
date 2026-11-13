//! Workspace error definitions and domain error conversions.

use serde::Serialize;
use specta::Type;

/// A workspace failure returned to the application.
#[derive(Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceError {
    /// A stable code that identifies the failure.
    pub code: &'static str,
    /// A message that explains the failure to the user.
    pub message: String,
}

/// The result of an LGTM workspace operation.
pub type Result<T> = std::result::Result<T, WorkspaceError>;

/// Builds a stable workspace failure for services and transport adapters.
pub fn error(code: &'static str, message: impl Into<String>) -> WorkspaceError {
    WorkspaceError {
        code,
        message: message.into(),
    }
}

impl From<std::io::Error> for WorkspaceError {
    fn from(value: std::io::Error) -> Self {
        error("io", value.to_string())
    }
}

impl From<lgtm_project::LgtmProjectError> for WorkspaceError {
    fn from(value: lgtm_project::LgtmProjectError) -> Self {
        use lgtm_project::LgtmProjectError;
        match value {
            LgtmProjectError::InvalidBranch(msg) => error("invalidBranch", msg),
            LgtmProjectError::BranchAlreadyCheckedOut(msg) => error("branchAlreadyCheckedOut", msg),
            LgtmProjectError::CheckoutPathExists(msg) => error("checkoutPathExists", msg),
            LgtmProjectError::NotGitRepository(msg) => error("notGitRepository", msg),
            LgtmProjectError::DirtyWorktree(msg) => error("dirtyWorktree", msg),
            LgtmProjectError::InvalidPath(msg) => error("invalidWorkspace", msg),
            LgtmProjectError::NotFound(msg) => error("notFound", msg),
            LgtmProjectError::Git(msg) => error("git", msg),
        }
    }
}

impl From<lgtm_session::LgtmSessionError> for WorkspaceError {
    fn from(value: lgtm_session::LgtmSessionError) -> Self {
        use lgtm_session::LgtmSessionError;
        match value {
            LgtmSessionError::Project(err) => err.into(),
            LgtmSessionError::NotFound(msg) => error("notFound", msg),
            other => error("session", other.to_string()),
        }
    }
}
