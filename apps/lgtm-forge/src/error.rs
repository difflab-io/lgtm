//! Application error definitions and Connect RPC status mappings.

use crate::database::DbError;

/// Convenience result alias for `lgtm-forge` operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Domain, Git, remote forge, and persistence errors produced by `lgtm-forge`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Input failed domain validation or violated a context invariant.
    #[error("{0}")]
    Invalid(String),

    /// Requested review, session, comment, or Git object was not found.
    #[error("{0}")]
    NotFound(String),

    /// Operation conflicted with existing review, session, or anchor state.
    #[error("{0}")]
    Conflict(String),

    /// Local Git repository or snapshot operation failed.
    #[error("git error: {0}")]
    Git(String),

    /// Remote forge API communication or synchronization failed.
    #[error("forge error: {0}")]
    Forge(String),

    /// Underlying persistence failure in `lgtm-db`.
    #[error("database error: {0}")]
    Database(String),
}

impl From<DbError> for Error {
    fn from(error: DbError) -> Self {
        match error {
            DbError::InvalidInput(msg) => Self::Invalid(msg),
            DbError::NotFound { entity, key } => {
                Self::NotFound(format!("{entity} not found: {key}"))
            }
            DbError::Conflict(msg) => Self::Conflict(msg),
            DbError::Database(err) => Self::Database(err.to_string()),
        }
    }
}

impl From<Error> for connectrpc::ConnectError {
    fn from(error: Error) -> Self {
        match error {
            Error::Invalid(message) => Self::invalid_argument(message),
            Error::NotFound(message) => Self::not_found(message),
            Error::Conflict(message) => Self::failed_precondition(message),
            Error::Git(message) | Error::Forge(message) | Error::Database(message) => {
                Self::internal(message)
            }
        }
    }
}
