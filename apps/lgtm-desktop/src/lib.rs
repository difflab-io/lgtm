//! LGTM desktop application shell, IPC command handlers, and local workspace, diff, review, and LSP services.

#![warn(missing_docs, unreachable_pub)]

mod app;
mod error;
mod events;
mod menus;
mod models;
mod services;

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
pub use menus::build_app_menu;
pub use menus::handle_menu_event;
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
