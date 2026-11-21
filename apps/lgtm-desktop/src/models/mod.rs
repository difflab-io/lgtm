//! Domain models for desktop workspace diff inspection and review mode state.

pub(crate) mod diff;

pub use diff::DiffCommitItem;
pub use diff::DiffCommitSource;
pub use diff::DiffFileEntry;
pub use diff::DiffFileStatus;
pub use diff::DiffWorkspaceSnapshot;
pub use diff::FileDiffContent;
pub use diff::ReviewMode;
