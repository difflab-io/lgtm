//! Desktop domain and host services for workspace filesystem access, diff resolution, review mode state, and LSP orchestration.

pub(crate) mod diff;
pub(crate) mod lsp;
pub(crate) mod review;
pub(crate) mod workspace;

pub use diff::DiffService;
pub use lsp::LspService;
pub use review::ReviewService;
pub use workspace::WorkspaceService;
