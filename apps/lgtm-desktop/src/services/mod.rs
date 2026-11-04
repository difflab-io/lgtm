#[cfg(any(test, feature = "e2e-bridge"))]
pub mod ipc_bridge;
pub mod workspace;

#[cfg(any(test, feature = "e2e-bridge"))]
pub use ipc_bridge::{BridgeEventForwarder, IpcBridgeService};
pub use workspace::WorkspaceService;
