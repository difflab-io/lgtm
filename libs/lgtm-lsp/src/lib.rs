//! Language Server Protocol (LSP) stdio client and workspace diagnostics orchestration.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LspError;
pub use error::Result;
pub use models::cache::LspCachedDocument;
pub use models::cache::LspDiscoveryCache;
pub use models::cache::LspDocumentSyncAction;
pub use models::cache::LspSessionKey;
pub use models::lsp::LspCompletionItem;
pub use models::lsp::LspDiagnostic;
pub use models::lsp::LspDiagnosticSeverity;
pub use models::lsp::LspDiagnosticsBuffer;
pub use models::lsp::LspEvent;
pub use models::lsp::LspFileDiagnosticsEvent;
pub use models::lsp::LspHoverResult;
pub use models::lsp::LspLocation;
pub use models::lsp::LspServerStatus;
pub use models::lsp::LspServerStatusEvent;
pub use models::lsp::LspSignatureHelp;
pub use models::lsp::LspSignatureInformation;
pub use models::lsp::LspWorkspaceSnapshot;
pub use services::cache::LspCacheService;
pub use services::manager::LgtmLspManager;
pub use services::manager::LspClientHandle;
