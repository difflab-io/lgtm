use std::path::PathBuf;

use lgtm_config::LgtmConfig;
use serde::{Deserialize, Serialize};

/// Composite key identifying an LSP server session by `(workspace_root, language_id, project_root)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSessionKey {
    /// Canonical workspace root path.
    pub workspace_root: PathBuf,
    /// Language identifier (`rust`, `typescript`, `python`, etc.).
    pub language_id: String,
    /// Canonical project root path resolved from language root markers.
    pub project_root: PathBuf,
}

impl LspSessionKey {
    /// Creates a new `LspSessionKey` for `workspace_root`, `language_id`, and `project_root`.
    pub fn new(
        workspace_root: impl Into<PathBuf>,
        language_id: impl Into<String>,
        project_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            language_id: language_id.into(),
            project_root: project_root.into(),
        }
    }
}

/// Cached state of an open editor document synchronized with a language server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspCachedDocument {
    /// Canonical workspace root path containing the document.
    pub workspace_root: PathBuf,
    /// Canonical project root path resolved for the document's language server.
    pub project_root: PathBuf,
    /// Language identifier associated with the document.
    pub language_id: String,
    /// Monotonically increasing LSP document version number (`1..`).
    pub version: i32,
    /// Latest UTF-8 text synchronized with the language server.
    pub text: String,
}

/// Outcome of synchronizing a document buffer in `LspCacheService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspDocumentSyncAction {
    /// The document was opened for the first time at version `1`.
    Opened(LspCachedDocument),
    /// The document text changed and its version was incremented.
    Changed(LspCachedDocument),
    /// The document was already tracked with identical text.
    Unchanged(LspCachedDocument),
}

/// On-disk and in-memory record of discovered workspace languages and merged LGTM configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspDiscoveryCache {
    /// Canonical workspace root directory path.
    pub workspace_root: String,
    /// Sorted list of detected language identifiers in the workspace.
    pub detected_languages: Vec<String>,
    /// Merged global and workspace LGTM configuration.
    pub config: LgtmConfig,
}
