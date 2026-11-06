use std::path::PathBuf;

use thiserror::Error;

/// Errors returned while loading or validating LGTM global or project configuration.
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
    /// Requested workspace directory does not exist on disk.
    #[error("Workspace directory '{}' does not exist", path.display())]
    WorkspaceDirectoryNotFound {
        /// Requested workspace path.
        path: PathBuf,
    },
    /// Requested workspace path exists but is not a directory.
    #[error("Workspace path '{}' is not a directory", path.display())]
    WorkspacePathNotDirectory {
        /// Requested workspace path.
        path: PathBuf,
    },
    /// Failed to canonicalize a workspace directory path.
    #[error("Canonicalize workspace directory '{}': {source}", path.display())]
    WorkspaceCanonicalize {
        /// Requested workspace path.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to locate the `lgtm-desktop` executable on disk or in `PATH`.
    #[error("{detail}")]
    DesktopExecutableNotFound {
        /// Human-readable diagnostic describing the missing desktop binary.
        detail: String,
    },
}

/// Result alias for `lgtm-config` operations.
pub type Result<T> = std::result::Result<T, LgtmConfigError>;
