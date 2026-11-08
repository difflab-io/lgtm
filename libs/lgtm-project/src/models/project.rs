//! Domain models representing projects, linked Git worktrees, and worktree ownership.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ownership classification determining whether LGTM manages and deletes a worktree's disk storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeSource {
    /// Created and owned by LGTM under `~/.lgtm/worktrees/<project>/<branch-slug>`.
    Managed,
    /// Created outside `~/.lgtm/worktrees` (such as `~/.herdr/worktrees/<project>/<branch-slug>`).
    External,
}

/// Linked Git worktree checkout grouped underneath a parent [`Project`].
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
        let name = branch
            .as_deref()
            .map(|b| b.strip_prefix("worktree/").unwrap_or(b).to_owned())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| {
                std::path::Path::new(&path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&path)
                    .to_owned()
            });
        Self {
            path,
            name,
            branch,
            source,
        }
    }
}

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
        let name = std::path::Path::new(&root_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&root_path)
            .to_owned();
        Self {
            id,
            name,
            root_path,
            branch,
            git_common_dir,
            collapsed: false,
            worktrees,
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns whether `workspace_root` matches this project's `root_path` or one of its open `worktrees`.
    pub fn contains_workspace(&self, workspace_root: &str) -> bool {
        self.root_path == workspace_root
            || self.worktrees.iter().any(|wt| wt.path == workspace_root)
    }

    /// Inserts or updates a linked worktree in `self.worktrees`, preserving existing order.
    pub fn upsert_worktree(&mut self, worktree: Worktree) {
        if let Some(existing) = self
            .worktrees
            .iter_mut()
            .find(|wt| wt.path == worktree.path)
        {
            *existing = worktree;
        } else {
            self.worktrees.push(worktree);
        }
    }
}
