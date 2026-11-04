use std::path::PathBuf;

use thiserror::Error;

/// Errors returned while loading or synchronizing LGTM global, project, or Mise configuration.
#[derive(Debug, Error)]
pub enum LgtmConfigError {
    /// Failed to read or write the global configuration file (`~/.lgtm/config.toml`).
    #[error("failed to read global configuration at {path}: {source}")]
    GlobalConfigIo {
        /// Path to the global configuration file.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse the global configuration file (`~/.lgtm/config.toml`).
    #[error("failed to parse global configuration at {path}: {source}")]
    GlobalConfigParse {
        /// Path to the global configuration file.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: Box<toml::de::Error>,
    },
    /// Failed to read or write a project configuration file (`<workspace>/.lgtm/config.toml`).
    #[error("failed to access project configuration at {path}: {source}")]
    ProjectConfigIo {
        /// Path to the project configuration file or `.lgtm` directory.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse a project configuration file (`<workspace>/.lgtm/config.toml`).
    #[error("failed to parse project configuration at {path}: {source}")]
    ProjectConfigParse {
        /// Path to the project configuration file.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: Box<toml::de::Error>,
    },
    /// Failed to read or write the LGTM-managed Mise file (`<workspace>/.lgtm/mise.toml`).
    #[error("failed to write managed Mise configuration at {path}: {source}")]
    ManagedMiseIo {
        /// Path to the managed `.lgtm/mise.toml` file.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to serialize configuration models to TOML.
    #[error("failed to serialize configuration: {0}")]
    SerializeToml(#[from] toml::ser::Error),
    /// A `mise` CLI invocation failed.
    #[error("mise command failed: {0}")]
    MiseFailed(String),
}

/// Result alias for `lgtm-config` operations.
pub type Result<T> = std::result::Result<T, LgtmConfigError>;
