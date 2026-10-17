//! Errors returned by connection, repository and transaction operations.
/// A rejected operation, database failure or filesystem failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    /// Invalid client configuration or transaction ownership.
    Invalid(String),
    #[error(transparent)]
    /// Database execution, constraint, migration or connection failure.
    Sql(#[from] sea_orm::DbErr),
    #[error(transparent)]
    /// Filesystem failure while acquiring SQLite startup coordination.
    Io(#[from] std::io::Error),
}

/// Result of a persistence operation.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
