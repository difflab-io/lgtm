//! Domain model representing active left-dock sidebar widgets in a desktop workspace.

use serde::{Deserialize, Serialize};

/// Left-sidebar widget mode persisted for an active workspace shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SidebarWidget {
    /// Active workspace file explorer (`FileExplorer.svelte`).
    #[default]
    Explorer,
    /// Open projects and collapsible worktrees tree (`ProjectsPane.svelte`).
    Projects,
}
