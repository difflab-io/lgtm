//! Domain model for persisted LGTM sessions orchestrating project and workspace state.

use std::collections::BTreeMap;

use lgtm_project::Project;
use lgtm_workspace::{RightDockWidget, SidebarWidget, WorkspaceSessionState};
use serde::{Deserialize, Serialize};

/// Top-level persisted session snapshot stored at `~/.lgtm/sessions/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Session identifier (`"default"` by default).
    pub id: String,
    /// Active left-sidebar widget (`Explorer` or `Projects`).
    #[serde(default)]
    pub sidebar_widget: SidebarWidget,
    /// Whether the right dock is open across workspaces and on app launch.
    #[serde(default)]
    pub right_dock_open: bool,
    /// Active widget hosted inside `RightDock` (`Terminal` by default).
    #[serde(default)]
    pub right_dock_widget: RightDockWidget,
    /// Persisted pixel width of `RightDock`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_dock_width: Option<u32>,
    /// Persisted pixel height of `AgentsPane` in the bottom section of `#projects-dock-pane`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_pane_height: Option<u32>,
    /// Canonical path of the currently active workspace checkout.
    pub active_workspace_root: String,
    /// Ordered list of open projects and their linked worktrees.
    #[serde(default)]
    pub projects: Vec<Project>,
    /// Remembered editor tabs and explorer state keyed by checkout path.
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceSessionState>,
}

impl Session {
    /// Creates a new empty session with identifier `id`.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.trim().to_owned(),
            sidebar_widget: SidebarWidget::Explorer,
            right_dock_open: false,
            right_dock_widget: RightDockWidget::Terminal,
            right_dock_width: None,
            agents_pane_height: None,
            active_workspace_root: String::new(),
            projects: Vec::new(),
            workspaces: BTreeMap::new(),
        }
    }

    // Public ------------------------------------------------------------------

    /// Sets the active workspace checkout path and ensures an entry exists in `self.workspaces`.
    pub fn set_active_workspace(&mut self, workspace_root: String) {
        self.active_workspace_root = workspace_root.clone();
        self.workspaces
            .entry(workspace_root.clone())
            .or_insert_with(|| WorkspaceSessionState::new(workspace_root, Vec::new(), Vec::new()));
    }

    /// Updates `self.workspaces` with a prepared [`WorkspaceSessionState`].
    pub fn set_workspace_state(&mut self, state: WorkspaceSessionState) {
        self.workspaces.insert(state.workspace_root.clone(), state);
    }

    /// Cleans up workspace state and falls back `active_workspace_root` to `parent_root` when a worktree is removed.
    pub fn apply_worktree_removal(
        &mut self,
        canonical_path: &str,
        raw_path: &str,
        parent_root: String,
    ) {
        self.workspaces.remove(canonical_path);
        self.workspaces.remove(raw_path);
        if self.active_workspace_root == canonical_path || self.active_workspace_root == raw_path {
            self.active_workspace_root = parent_root;
        }
    }

    /// Cleans up workspace state and updates `active_workspace_root` when `removed` is detached from the session.
    pub fn apply_project_removal(&mut self, removed: &Project) {
        self.workspaces.remove(&removed.root_path);
        for wt in &removed.worktrees {
            self.workspaces.remove(&wt.path);
        }
        if removed.has_workspace(&self.active_workspace_root) {
            self.active_workspace_root = self
                .projects
                .first()
                .map(|p| p.root_path.clone())
                .unwrap_or_default();
        }
    }
}
