#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] super::database::error::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    /// The final review event was explicitly rejected after all comment writes were confirmed.
    #[error("{0}")]
    ReviewRejected(String),
    #[error(transparent)]
    Sql(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

impl From<Error> for connectrpc::ConnectError {
    fn from(error: Error) -> Self {
        use connectrpc::ConnectError;

        match error {
            crate::error::Error::Invalid(message) => ConnectError::invalid_argument(message),
            crate::error::Error::NotFound(message) => ConnectError::not_found(message),
            crate::error::Error::Conflict(message) => ConnectError::failed_precondition(message),
            other => {
                tracing::error!(error = %other, "RPC failed");
                ConnectError::internal("Backend operation failed")
            }
        }
    }
}
