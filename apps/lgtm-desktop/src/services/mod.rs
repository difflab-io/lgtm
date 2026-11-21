#[cfg(any(test, feature = "e2e-bridge"))]
pub mod ipc_bridge;
pub mod lsp;

#[cfg(any(test, feature = "e2e-bridge"))]
pub use ipc_bridge::IpcBridgeService;
pub use lsp::LspService;
