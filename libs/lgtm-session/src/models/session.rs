//! Domain models for persisted LGTM sessions and dock sidebar widgets.

use std::collections::BTreeMap;

use lgtm_project::Project;
use serde::{Deserialize, Serialize};

use crate::models::{tab::PersistedTab, workspace::WorkspaceSessionState};

/// Left-sidebar widget mode persisted in a [`Session`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SidebarWidget {
    /// Active workspace file explorer (`FileExplorer.svelte`).
    #[default]
    Explorer,
    /// Open projects and collapsible worktrees tree (`ProjectsPane.svelte`).
    Projects,
}

/// Top-level persisted session snapshot stored at `~/.lgtm/sessions/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Session identifier (`"default"` by default).
    pub id: String,
    /// Active left-sidebar widget (`Explorer` or `Projects`).
    #[serde(default)]
    pub sidebar_widget: SidebarWidget,
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

    /// Updates `self.workspaces[workspace_root]` with `expanded_dirs` and `retained_tabs`.
    pub fn set_workspace_state(
        &mut self,
        workspace_root: String,
        expanded_dirs: Vec<String>,
        retained_tabs: Vec<PersistedTab>,
    ) {
        self.workspaces.insert(
            workspace_root.clone(),
            WorkspaceSessionState::new(workspace_root, expanded_dirs, retained_tabs),
        );
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
