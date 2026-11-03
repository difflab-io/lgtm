use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration at {path}: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize configuration: {0}")]
    SerializeToml(#[from] toml::ser::Error),
    #[error("mise command failed: {0}")]
    MiseFailed(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
