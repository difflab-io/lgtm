//! Local workspace filesystem service, entry validation, tab retention, and widget/UI state models for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::Result;
pub use error::WorkspaceError;
pub use error::error;
pub use models::tab::PersistedTab;
pub use models::tab::TabCandidate;
pub use models::widget::SidebarWidget;
pub use models::workspace::Workspace;
pub use models::workspace::WorkspaceListing;
pub use models::workspace::WorkspaceNode;
pub use models::workspace_state::WorkspaceSessionState;
pub use models::workspace_state::WorkspaceStatePatch;
pub use services::workspace::WorkspaceService;
