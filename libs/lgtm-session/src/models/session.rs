//! Domain models for persisted LGTM sessions, sidebar widgets, and per-workspace editor state.

use std::collections::BTreeMap;

use lgtm_project::Project;
use serde::{Deserialize, Serialize};

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

/// Candidate editor tab evaluated against the session retention filter (`edited || last_opened`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TabCandidate {
    /// Workspace-relative file path or config path (`~/.lgtm/config.json` / `.lgtm/config.json`).
    pub path: String,
    /// Display title shown in the tab bar.
    pub title: String,
    /// Whether the tab buffer was modified during this session.
    pub edited: bool,
    /// Whether the tab was the active focused tab when the workspace was saved or switched.
    pub last_opened: bool,
}

/// Editor tab persisted in [`WorkspaceSessionState`] after passing the retention filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PersistedTab {
    /// Workspace-relative file path or config path.
    pub path: String,
    /// Display title shown in the tab bar.
    pub title: String,
    /// Whether the tab buffer was modified during the session.
    pub edited: bool,
    /// Whether the tab was the active focused tab when saved.
    pub last_opened: bool,
}

impl PersistedTab {
    // Public ------------------------------------------------------------------

    /// Filters `candidates` so only tabs with `edited == true` or `last_opened == true` are retained.
    pub fn retain_from_candidates(candidates: Vec<TabCandidate>) -> Vec<Self> {
        candidates
            .into_iter()
            .filter(|tab| !tab.path.trim().is_empty() && (tab.edited || tab.last_opened))
            .map(|tab| Self {
                path: tab.path,
                title: tab.title,
                edited: tab.edited,
                last_opened: tab.last_opened,
            })
            .collect()
    }
}

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

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_open_tab_candidates {
        use super::*;

        mod when_retaining_tabs_for_persistence {
            use super::*;

            #[test]
            fn should_keep_only_edited_tabs_and_the_last_opened_tab() {
                // Arrange
                let candidates = vec![
                    TabCandidate {
                        path: "README.md".to_owned(),
                        title: "README.md".to_owned(),
                        edited: false,
                        last_opened: false,
                    },
                    TabCandidate {
                        path: "src/lib.rs".to_owned(),
                        title: "lib.rs".to_owned(),
                        edited: true,
                        last_opened: false,
                    },
                    TabCandidate {
                        path: "src/app.rs".to_owned(),
                        title: "app.rs".to_owned(),
                        edited: false,
                        last_opened: true,
                    },
                ];

                // Act
                let retained = PersistedTab::retain_from_candidates(candidates);

                // Assert
                assert_eq!(retained.len(), 2);
                assert_eq!(retained[0].path, "src/lib.rs");
                assert!(retained[0].edited);
                assert_eq!(retained[1].path, "src/app.rs");
                assert!(retained[1].last_opened);
            }
        }
    }
}
