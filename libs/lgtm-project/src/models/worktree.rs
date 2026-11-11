//! Domain model representing a managed Git worktree checkout under a [`crate::Project`].

use serde::{Deserialize, Serialize};

/// Managed Git worktree checkout grouped underneath a parent [`crate::Project`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Canonical filesystem path of the managed worktree directory.
    pub path: String,
    /// Display label for the worktree (branch name stripped of `worktree/`, or directory basename).
    pub name: String,
    /// Checked-out Git branch name, if not detached.
    pub branch: Option<String>,
}

impl Worktree {
    /// Constructs a managed worktree model and normalizes its display label from `branch` or `path`.
    pub fn new(path: String, branch: Option<String>) -> Self {
        let name = Self::default_name(&path, branch.as_deref());
        Self { path, name, branch }
    }

    // Public ------------------------------------------------------------------

    /// Computes the default display label for a worktree from its `branch` (stripped of `worktree/`) or directory basename.
    pub fn default_name(path: &str, branch: Option<&str>) -> String {
        branch
            .map(|b| b.strip_prefix("worktree/").unwrap_or(b).to_owned())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path)
                    .to_owned()
            })
    }

    /// Returns whether `self.name` has been customized away from the default branch/path label.
    pub fn has_custom_name(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name != Self::default_name(&self.path, self.branch.as_deref())
    }
}
