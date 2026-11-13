//! Workspace session persistence, project tree state, and per-checkout tab retention for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmSessionError;
pub use error::Result;
pub use models::session::Session;
pub use models::session::SidebarWidget;
pub use models::tab::PersistedTab;
pub use models::tab::TabCandidate;
pub use models::workspace::WorkspaceSessionState;
pub use models::workspace::WorkspaceStatePatch;
pub use services::session::SessionService;
