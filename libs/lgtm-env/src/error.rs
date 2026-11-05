use std::path::PathBuf;

use thiserror::Error;

/// Errors returned while managing the `mise` binary, synthesizing `.lgtm/mise.toml`, or executing Mise commands.
#[derive(Debug, Error)]
pub enum LgtmEnvError {
    /// Failed to load or validate workspace or global LGTM configuration.
    #[error(transparent)]
    Config(#[from] lgtm_config::LgtmConfigError),
    /// Failed to read or write the LGTM-managed Mise file (`<workspace>/.lgtm/mise.toml`).
    #[error("failed to write managed Mise configuration at {path}: {source}")]
    ManagedMiseIo {
        /// Path to the managed `.lgtm/mise.toml` file.
        path: PathBuf,
        /// Underlying filesystem I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to serialize generated Mise models to TOML.
    #[error("failed to serialize Mise configuration: {0}")]
    SerializeToml(#[from] toml::ser::Error),
    /// No usable `mise` binary was found and automatic installation was not performed.
    #[error("mise binary is not installed")]
    MiseNotInstalled,
    /// Automatic installation of the `mise` binary into `<lgtm_home>/bin/mise` failed.
    #[error("failed to install mise binary: {0}")]
    MiseInstallFailed(String),
    /// A `mise` CLI invocation failed.
    #[error("mise command failed: {0}")]
    MiseFailed(String),
}

/// Result alias for `lgtm-env` operations.
pub type Result<T> = std::result::Result<T, LgtmEnvError>;
