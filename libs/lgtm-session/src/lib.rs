//! Workspace session persistence, project tree state, and per-checkout tab retention for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmSessionError;
pub use error::Result;
pub use models::session::PersistedTab;
pub use models::session::Session;
pub use models::session::SidebarWidget;
pub use models::session::TabCandidate;
pub use models::session::WorkspaceSessionState;
pub use models::session::WorkspaceStatePatch;
pub use services::session::SessionService;
