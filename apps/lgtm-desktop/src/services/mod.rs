//! Desktop domain and host services for workspace filesystem access, diff resolution, review mode state, and LSP orchestration.

pub mod diff;
#[cfg(any(test, feature = "e2e-bridge"))]
pub mod ipc_bridge;
pub mod lsp;
pub mod review;
pub mod workspace;

pub use diff::DiffService;
#[cfg(any(test, feature = "e2e-bridge"))]
pub use ipc_bridge::IpcBridgeService;
pub use lsp::LspService;
pub use review::ReviewService;
pub use workspace::WorkspaceService;
