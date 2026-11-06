use std::path::PathBuf;

use thiserror::Error;

/// Errors returned while loading, validating, or persisting LGTM global or project configuration.
#[derive(Debug, Error)]
pub enum LgtmConfigError {
    /// Failed to read or write the global configuration file (`~/.lgtm/config.{json,yaml,yml}`).
    #[error("failed to access global configuration at {path}: {source}")]
    GlobalConfigIo {
        /// Path to the global configuration file.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse or serialize the global configuration file (`~/.lgtm/config.{json,yaml,yml}`).
    #[error("failed to parse global configuration at {path}: {message}")]
    GlobalConfigParse {
        /// Path to the global configuration file.
        path: PathBuf,
        /// Human-readable JSON or YAML parse error message.
        message: String,
    },
    /// Failed to read or write a project configuration file (`<workspace>/.lgtm/config.{json,yaml,yml}`).
    #[error("failed to access project configuration at {path}: {source}")]
    ProjectConfigIo {
        /// Path to the project configuration file or `.lgtm` directory.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse or serialize a project configuration file (`<workspace>/.lgtm/config.{json,yaml,yml}`).
    #[error("failed to parse project configuration at {path}: {message}")]
    ProjectConfigParse {
        /// Path to the project configuration file.
        path: PathBuf,
        /// Human-readable JSON or YAML parse error message.
        message: String,
    },
}

/// Result alias for `lgtm-config` operations.
pub type Result<T> = std::result::Result<T, LgtmConfigError>;
