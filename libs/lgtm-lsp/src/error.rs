use thiserror::Error;

#[derive(Debug, Error)]
pub enum LspError {
    #[error(transparent)]
    Config(#[from] lgtm_config::ConfigError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-RPC error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("LSP server unavailable: {0}")]
    ServerUnavailable(String),
    #[error("LSP protocol error: {0}")]
    Protocol(String),
    #[error("LSP request timed out: {0}")]
    Timeout(String),
}

pub type Result<T> = std::result::Result<T, LspError>;
