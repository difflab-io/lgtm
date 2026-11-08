//! LGTM desktop application shell, IPC command handlers, and local workspace, diff, review, and LSP services.

mod app;
pub mod error;
pub mod events;
pub mod menus;
pub mod models;
pub mod services;

pub use app::AppState;
pub use app::builder;
pub use app::commands;
pub use app::configure_builder;
pub use app::export_typescript_bindings_to;
pub use app::handle_event;
pub use app::run;
pub use app::specta_builder;
pub use error::Result;
pub use error::WorkspaceError;
pub use events::AppEvent;
pub use events::AppEvents;
pub use events::FileOpenedEvent;
pub use models::DiffCommitItem;
pub use models::DiffCommitSource;
pub use models::DiffFileEntry;
pub use models::DiffFileStatus;
pub use models::DiffWorkspaceSnapshot;
pub use models::FileContent;
pub use models::FileDiffContent;
pub use models::ReviewMode;
pub use models::Workspace;
pub use models::WorkspaceListing;
pub use models::WorkspaceNode;
pub use services::DiffService;
#[cfg(any(test, feature = "e2e-bridge"))]
pub use services::IpcBridgeService;
pub use services::LspService;
pub use services::ReviewService;
pub use services::WorkspaceService;
