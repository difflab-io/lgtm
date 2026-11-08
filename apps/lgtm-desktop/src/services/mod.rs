//! Desktop domain and host services for workspace filesystem access, diff resolution, review mode state, and LSP orchestration.

pub(crate) mod diff;
#[cfg(any(test, feature = "e2e-bridge"))]
pub(crate) mod ipc_bridge;
pub(crate) mod lsp;
pub(crate) mod review;
pub(crate) mod workspace;

pub use diff::DiffService;
#[cfg(any(test, feature = "e2e-bridge"))]
pub use ipc_bridge::IpcBridgeService;
pub use lsp::LspService;
pub use review::ReviewService;
pub use workspace::WorkspaceService;
