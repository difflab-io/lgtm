//! Local workspace filesystem service, entry validation, and UTF-8 document I/O for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::Result;
pub use error::WorkspaceError;
pub use error::error;
pub use models::file_content::FileContent;
pub use models::workspace::Workspace;
pub use models::workspace::WorkspaceListing;
pub use models::workspace::WorkspaceNode;
pub use services::workspace::WorkspaceService;
