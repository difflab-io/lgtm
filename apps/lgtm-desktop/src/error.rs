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
