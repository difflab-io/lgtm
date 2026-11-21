//! LGTM desktop domain models and workspace, diff, review, and LSP services.

#![warn(missing_docs, unreachable_pub)]

mod models;
mod services;

pub use lgtm_workspace::Result;
pub use lgtm_workspace::Workspace;
pub use lgtm_workspace::WorkspaceError;
pub use lgtm_workspace::WorkspaceListing;
pub use lgtm_workspace::WorkspaceNode;
pub use lgtm_workspace::WorkspaceService;
pub use lgtm_workspace::error;
pub use models::DiffCommitItem;
pub use models::DiffCommitSource;
pub use models::DiffFileEntry;
pub use models::DiffFileStatus;
pub use models::DiffWorkspaceSnapshot;
pub use models::FileDiffContent;
pub use models::ReviewMode;
pub use services::DiffService;
pub use services::LspService;
pub use services::ReviewService;
