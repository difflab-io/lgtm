//! Service resolving project roots and managing LGTM-owned Git worktrees.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use lgtm_config::ConfigService;
use lgtm_rsutils::{
    add_worktree, branch_to_path_slug, get_current_branch, remove_worktree, resolve_git_paths,
};

use crate::{
    error::{LgtmProjectError, Result},
    models::{project::Project, worktree::Worktree},
};

/// Resolves project checkouts and manages `~/.lgtm/worktrees/<project>/<branch-slug>` worktrees.
#[derive(Debug, Clone)]
pub struct ProjectService {
    worktrees_root: PathBuf,
}

impl ProjectService {
    /// Creates a project service storing managed worktrees under `<lgtm_home>/worktrees` (or `~/.lgtm/worktrees`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(|| ConfigService::new().get_lgtm_home_dir());
        Self {
            worktrees_root: base.join("worktrees"),
        }
    }

    // Public ------------------------------------------------------------------

    /// Resolves any directory path into its parent [`Project`] (primary checkout) and optional managed [`Worktree`].
    pub fn resolve_project(&self, path: &Path) -> Result<(Project, Option<Worktree>)> {
        let canonical = std::fs::canonicalize(path).map_err(|err| {
            tracing::warn!(path = %path.display(), error = %err, "failed to canonicalize project path");
            LgtmProjectError::InvalidPath(format!(
                "failed to resolve project path '{}': {err}",
                path.display()
            ))
        })?;
        if !canonical.is_dir() {
            tracing::warn!(path = %canonical.display(), "project path is not a directory");
            return Err(LgtmProjectError::InvalidPath(format!(
                "'{}' is not a directory.",
                canonical.display()
            )));
        }

        if let Ok(resolved) = resolve_git_paths(&canonical)
            && resolved.common_git_dir.exists()
            && resolved.common_git_dir != canonical
        {
            let main_root_str = resolved.main_root.to_string_lossy().into_owned();
            let main_branch = get_current_branch(&resolved.main_root);
            let common_git_str = Some(resolved.common_git_dir.to_string_lossy().into_owned());
            let mut project = Project::new(main_root_str, main_branch, common_git_str, Vec::new());

            if resolved.worktree_root != resolved.main_root
                && self.is_managed_worktree(&project.name, &resolved.worktree_root)
            {
                let wt_path_str = resolved.worktree_root.to_string_lossy().into_owned();
                let wt_branch = get_current_branch(&resolved.worktree_root);
                let worktree = Worktree::new(wt_path_str, wt_branch);
                project.upsert_worktree(worktree.clone());
                tracing::info!(
                    project = %project.name,
                    worktree = %worktree.path,
                    "resolved managed git worktree under parent project"
                );
                return Ok((project, Some(worktree)));
            }
            tracing::info!(
                project = %project.name,
                root_path = %project.root_path,
                branch = ?project.branch,
                "resolved primary git project checkout"
            );
            return Ok((project, None));
        }

        let root_str = canonical.to_string_lossy().into_owned();
        let project = Project::new(root_str, None, None, Vec::new());
        tracing::info!(
            project = %project.name,
            root_path = %project.root_path,
            "resolved non-git directory project"
        );
        Ok((project, None))
    }

    /// Refreshes branch labels and prunes worktrees whose directories no longer exist while preserving custom worktree section names.
    pub fn refresh_project(&self, project: &mut Project) -> Result<()> {
        let root_path = Path::new(&project.root_path);
        if root_path.is_dir() {
            project.branch = get_current_branch(root_path);
        }
        let dir_name = Project::default_name(&project.root_path);
        project.worktrees.retain(|wt| {
            let wt_path = Path::new(&wt.path);
            wt_path.is_dir() && self.is_managed_worktree(&dir_name, wt_path)
        });
        for wt in &mut project.worktrees {
            let wt_path = Path::new(&wt.path);
            let branch = get_current_branch(wt_path);
            if wt.has_custom_name() {
                wt.branch = branch;
            } else {
                *wt = Worktree::new(wt.path.clone(), branch);
            }
        }
        tracing::debug!(
            project = %project.name,
            worktree_count = project.worktrees.len(),
            "refreshed project branch and worktree metadata"
        );
        Ok(())
    }

    /// Computes the managed checkout path (`~/.lgtm/worktrees/<project>/<branch-slug>`) for `branch`.
    pub fn get_worktree_path(&self, project: &Project, branch: &str) -> PathBuf {
        let slug = branch_to_path_slug(branch);
        let dir_name = Project::default_name(&project.root_path);
        self.worktrees_root.join(dir_name).join(slug)
    }

    /// Creates a managed Git worktree under `~/.lgtm/worktrees/<project>/<branch-slug>` for `project`.
    pub fn create_worktree(
        &self,
        project: &Project,
        branch: &str,
        base_ref: Option<&str>,
    ) -> Result<Worktree> {
        let root_path = Path::new(&project.root_path);
        if project.git_common_dir.is_none() && !root_path.join(".git").exists() {
            tracing::warn!(project = %project.name, "cannot create worktree on non-git project");
            return Err(LgtmProjectError::NotGitRepository(format!(
                "'{}' is not a Git repository.",
                project.name
            )));
        }
        let target_dir = self.get_worktree_path(project, branch);
        if target_dir.exists() {
            tracing::warn!(target = %target_dir.display(), "managed worktree path already exists");
            return Err(LgtmProjectError::CheckoutPathExists(format!(
                "Worktree path '{}' already exists.",
                target_dir.display()
            )));
        }
        let created = add_worktree(root_path, &target_dir, branch, base_ref).map_err(|err| {
            tracing::warn!(project = %project.name, branch = %branch, error = %err, "failed to create git worktree");
            match err.kind() {
                ErrorKind::InvalidInput => LgtmProjectError::InvalidBranch(err.to_string()),
                ErrorKind::AlreadyExists => {
                    LgtmProjectError::BranchAlreadyCheckedOut(err.to_string())
                }
                _ => LgtmProjectError::Git(err.to_string()),
            }
        })?;
        let path_str = created.path.to_string_lossy().into_owned();
        let worktree = Worktree::new(path_str, created.branch);
        tracing::info!(
            project = %project.name,
            worktree = %worktree.path,
            branch = ?worktree.branch,
            "created managed git worktree"
        );
        Ok(worktree)
    }

    /// Deletes disk storage for a managed worktree via `git worktree remove`.
    pub fn remove_worktree(
        &self,
        project: &Project,
        worktree_path: &Path,
        force: bool,
    ) -> Result<()> {
        let canonical =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        remove_worktree(Path::new(&project.root_path), &canonical, force).map_err(|err| {
            tracing::warn!(
                project = %project.name,
                worktree = %canonical.display(),
                force,
                error = %err,
                "failed to remove managed git worktree"
            );
            match err.kind() {
                ErrorKind::WouldBlock => LgtmProjectError::DirtyWorktree(err.to_string()),
                _ => LgtmProjectError::Git(err.to_string()),
            }
        })?;
        tracing::info!(
            project = %project.name,
            worktree = %canonical.display(),
            force,
            "removed managed git worktree from disk"
        );
        Ok(())
    }

    // Private -----------------------------------------------------------------

    fn is_managed_worktree(&self, project_name: &str, worktree_path: &Path) -> bool {
        let managed_base = std::fs::canonicalize(&self.worktrees_root)
            .unwrap_or_else(|_| self.worktrees_root.clone());
        let project_managed_dir = managed_base.join(project_name);
        let candidate =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        candidate.starts_with(&project_managed_dir) || candidate.starts_with(&managed_base)
    }
}
