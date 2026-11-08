//! LGTM desktop domain models and workspace, diff, review, and LSP services.

#![warn(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::Result;
pub use error::WorkspaceError;
pub use error::error;
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
pub use services::LspService;
pub use services::ReviewService;
pub use services::WorkspaceService;
