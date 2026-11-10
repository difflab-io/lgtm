//! Domain models for persisted LGTM sessions and dock sidebar widgets.

use std::collections::BTreeMap;

use lgtm_project::Project;
use serde::{Deserialize, Serialize};

use crate::models::workspace::WorkspaceSessionState;

/// Left-sidebar widget mode persisted in a [`Session`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SidebarWidget {
    /// Active workspace file explorer (`WorkspaceExplorer.svelte`).
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
}
