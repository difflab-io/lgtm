//! Domain models representing linked Git worktrees and worktree disk ownership.

use serde::{Deserialize, Serialize};

/// Ownership classification determining whether LGTM manages and deletes a worktree's disk storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeSource {
    /// Created and owned by LGTM under `~/.lgtm/worktrees/<project>/<branch-slug>`.
    Managed,
    /// Created outside `~/.lgtm/worktrees` (such as `~/.herdr/worktrees/<project>/<branch-slug>`).
    External,
}

/// Linked Git worktree checkout grouped underneath a parent [`crate::Project`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Canonical filesystem path of the linked worktree directory.
    pub path: String,
    /// Display label for the worktree (branch name stripped of `worktree/`, or directory basename).
    pub name: String,
    /// Checked-out Git branch name, if not detached.
    pub branch: Option<String>,
    /// Whether this worktree's disk storage is managed by LGTM or external.
    pub source: WorktreeSource,
}

impl Worktree {
    /// Constructs a linked worktree model and normalizes its display label from `branch` or `path`.
    pub fn new(path: String, branch: Option<String>, source: WorktreeSource) -> Self {
        let name = Self::default_name(&path, branch.as_deref());
        Self {
            path,
            name,
            branch,
            source,
        }
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
