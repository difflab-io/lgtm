//! Domain models for desktop workspace management, file buffers, and diff inspection.

pub(crate) mod diff;
pub(crate) mod file_content;
pub(crate) mod workspace;

pub use diff::DiffCommitItem;
pub use diff::DiffCommitSource;
pub use diff::DiffFileEntry;
pub use diff::DiffFileStatus;
pub use diff::DiffWorkspaceSnapshot;
pub use diff::FileDiffContent;
pub use diff::ReviewMode;
pub use file_content::FileContent;
pub use workspace::Workspace;
pub use workspace::WorkspaceListing;
pub use workspace::WorkspaceNode;
