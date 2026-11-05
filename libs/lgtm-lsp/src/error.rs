use thiserror::Error;

/// Errors returned by `lgtm-lsp` server lifecycle and JSON-RPC operations.
#[derive(Debug, Error)]
pub enum LspError {
    /// Failed to load or synchronize workspace LGTM configuration.
    #[error(transparent)]
    Config(#[from] lgtm_config::LgtmConfigError),
    /// Failed to manage the Mise environment or `.lgtm/mise.toml` file.
    #[error(transparent)]
    Env(#[from] lgtm_env::LgtmEnvError),
    /// Standard I/O failure while communicating with a language server process.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to serialize or deserialize a JSON-RPC message.
    #[error("JSON-RPC error: {0}")]
    Json(#[from] serde_json::Error),
    /// The requested language server binary could not be started or exited unexpectedly.
    #[error("LSP server unavailable: {0}")]
    ServerUnavailable(String),
    /// The language server returned a protocol or JSON-RPC error response.
    #[error("LSP protocol error: {0}")]
    Protocol(String),
    /// A JSON-RPC request exceeded the configured timeout.
    #[error("LSP request timed out: {0}")]
    Timeout(String),
}

/// Result alias for `lgtm-lsp` operations.
pub type Result<T> = std::result::Result<T, LspError>;
