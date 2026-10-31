//! Error contract and SQL constraint mapping for `lgtm-db`.

use sea_orm::DbErr;

/// Result alias for database client and repository operations.
pub type Result<T> = std::result::Result<T, DbError>;

/// Persistence error returned by `lgtm-db` client and repository methods.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// Target entity was not found by the supplied identifier or selector.
    #[error("{entity} not found: {key}")]
    NotFound {
        /// Entity type name (for example, `"review"` or `"comment"`).
        entity: &'static str,
        /// Lookup key or selector string.
        key: String,
    },

    /// Operation violated uniqueness or state-transition constraints.
    #[error("conflict: {0}")]
    Conflict(String),

    /// Input failed validation or SQL `CHECK` constraints.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Underlying SeaORM or SQL driver error.
    #[error("database error: {0}")]
    Database(#[from] DbErr),
}

impl DbError {
    // Public ------------------------------------------------------------------

    /// Construct a `NotFound` error for the named entity and key.
    pub fn not_found(entity: &'static str, key: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            key: key.into(),
        }
    }

    /// Construct a `Conflict` error with the given description.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// Construct an `InvalidInput` error with the given description.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    /// Map a SeaORM `DbErr` into a domain-specific `Conflict` or `InvalidInput` when caused by a SQL constraint.
    pub fn from_db_err(err: DbErr) -> Self {
        let text = err.to_string();
        let lower = text.to_ascii_lowercase();
        if lower.contains("unique constraint")
            || lower.contains("duplicate key")
            || lower.contains("23505")
        {
            Self::Conflict(text)
        } else if lower.contains("check constraint")
            || lower.contains("foreign key constraint")
            || lower.contains("23514")
            || lower.contains("23503")
        {
            Self::InvalidInput(text)
        } else {
            Self::Database(err)
        }
    }
}
