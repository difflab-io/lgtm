//! Domain model representing a primary repository or folder checkout and its open linked worktrees.

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::worktree::Worktree;

/// Primary repository or folder checkout and its open linked Git worktrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Deterministic UUID derived from `root_path`.
    #[specta(type = String)]
    pub id: Uuid,
    /// Repository or folder basename shown on the project row.
    pub name: String,
    /// Canonical path of the primary repository checkout.
    pub root_path: String,
    /// Checked-out Git branch at `root_path`, if `root_path` is a Git repository.
    pub branch: Option<String>,
    /// Canonical shared `.git` directory path (`commondir`), if `root_path` is a Git repository.
    pub git_common_dir: Option<String>,
    /// Optional custom icon identifier for `ProjectsPane` (defaults to `"box"` in the UI when `None`).
    #[serde(default)]
    pub icon: Option<String>,
    /// Optional custom color token identifier for `ProjectsPane` (defaults to neutral foreground when `None`).
    #[serde(default)]
    pub color: Option<String>,
    /// Whether child worktrees are collapsed in `ProjectsPane`.
    pub collapsed: bool,
    /// Open linked Git worktrees attached underneath this project.
    pub worktrees: Vec<Worktree>,
}

impl Project {
    /// Constructs a project rooted at `root_path` with a deterministic UUID v5 identifier.
    pub fn new(
        root_path: String,
        branch: Option<String>,
        git_common_dir: Option<String>,
        worktrees: Vec<Worktree>,
    ) -> Self {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, root_path.as_bytes());
        let name = Self::default_name(&root_path);
        Self {
            id,
            name,
            root_path,
            branch,
            git_common_dir,
            icon: None,
            color: None,
            collapsed: false,
            worktrees,
        }
    }

    // Public ------------------------------------------------------------------

    /// Computes the default display name for a project from its directory basename.
    pub fn default_name(root_path: &str) -> String {
        Path::new(root_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(root_path)
            .to_owned()
    }

    /// Returns whether `workspace_root` matches this project's `root_path` or one of its open `worktrees`.
    pub fn contains_workspace(&self, workspace_root: &str) -> bool {
        self.root_path == workspace_root
            || self.worktrees.iter().any(|wt| wt.path == workspace_root)
    }

    /// Updates custom `name`, `icon`, and `color` appearance properties on this project.
    pub fn update_appearance(
        &mut self,
        name: Option<String>,
        icon: Option<String>,
        color: Option<String>,
    ) {
        if let Some(raw_name) = name {
            let trimmed = raw_name.trim();
            self.name = if trimmed.is_empty() {
                Self::default_name(&self.root_path)
            } else {
                trimmed.to_owned()
            };
        }
        self.icon = icon.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        self.color = color.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    }

    /// Renames the display label of `worktree_path` without changing its Git branch name, returning `true` if found.
    pub fn rename_worktree(&mut self, worktree_path: &str, name: &str) -> bool {
        let Some(worktree) = self
            .worktrees
            .iter_mut()
            .find(|wt| wt.path == worktree_path)
        else {
            return false;
        };
        let trimmed = name.trim();
        worktree.name = if trimmed.is_empty() {
            Worktree::default_name(&worktree.path, worktree.branch.as_deref())
        } else {
            trimmed.to_owned()
        };
        true
    }

    /// Inserts or updates a linked worktree in `self.worktrees`, preserving existing order and any custom worktree section name.
    pub fn upsert_worktree(&mut self, worktree: Worktree) {
        if let Some(existing) = self
            .worktrees
            .iter_mut()
            .find(|wt| wt.path == worktree.path)
        {
            if existing.has_custom_name() {
                existing.branch = worktree.branch;
            } else {
                *existing = worktree;
            }
        } else {
            self.worktrees.push(worktree);
        }
    }
}
