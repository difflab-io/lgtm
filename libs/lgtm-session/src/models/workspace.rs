//! Domain models representing per-checkout workspace UI state and save patches.

use serde::{Deserialize, Serialize};

use crate::models::{
    session::SidebarWidget,
    tab::{PersistedTab, TabCandidate},
};

/// Per-checkout UI snapshot stored for a project primary checkout (`Project.root_path`) or linked worktree (`Worktree.path`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSessionState {
    /// Canonical path of the checkout directory.
    pub workspace_root: String,
    /// Path of the active tab when this workspace was last focused.
    pub last_opened_tab_path: Option<String>,
    /// Expanded directory paths in `WorkspaceExplorer`.
    pub expanded_dirs: Vec<String>,
    /// Retained editor tabs (`edited || last_opened`).
    pub tabs: Vec<PersistedTab>,
}

impl WorkspaceSessionState {
    /// Constructs a workspace session state with retained `tabs` and `expanded_dirs`.
    pub fn new(
        workspace_root: String,
        expanded_dirs: Vec<String>,
        tabs: Vec<PersistedTab>,
    ) -> Self {
        let last_opened_tab_path = tabs
            .iter()
            .find(|tab| tab.is_last_opened())
            .map(|tab| tab.path.clone());
        Self {
            workspace_root,
            last_opened_tab_path,
            expanded_dirs,
            tabs,
        }
    }
}

/// Patch payload submitted when saving or switching workspace UI state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatePatch {
    /// Canonical path of the workspace checkout being saved.
    pub workspace_root: String,
    /// Open editor tab candidates to filter and persist.
    pub tabs: Vec<TabCandidate>,
    /// Expanded directory paths in `WorkspaceExplorer`.
    pub expanded_dirs: Vec<String>,
    /// Optional updated sidebar widget selection (`Explorer` or `Projects`).
    pub sidebar_widget: Option<SidebarWidget>,
}
