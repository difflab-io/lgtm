//! Service resolving project roots and managing LGTM-owned and external Git worktrees.

use std::path::{Path, PathBuf};

use lgtm_rsutils::{
    RsUtilsError, add_worktree, branch_to_path_slug, default_lgtm_home, get_current_branch,
    remove_worktree, resolve_git_paths,
};

use crate::{
    error::{LgtmProjectError, Result},
    models::project::{Project, Worktree, WorktreeSource},
};

/// Resolves project checkouts and manages `~/.lgtm/worktrees/<project>/<branch-slug>` worktrees.
#[derive(Debug, Clone)]
pub struct ProjectService {
    worktrees_root: PathBuf,
}

impl ProjectService {
    /// Creates a project service storing managed worktrees under `<lgtm_home>/worktrees` (or `~/.lgtm/worktrees`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(default_lgtm_home);
        Self {
            worktrees_root: base.join("worktrees"),
        }
    }

    // Public ------------------------------------------------------------------

    /// Resolves any directory path into its parent [`Project`] (primary checkout) and optional linked [`Worktree`].
    pub fn resolve_project(&self, path: &Path) -> Result<(Project, Option<Worktree>)> {
        let canonical = std::fs::canonicalize(path).map_err(|err| {
            LgtmProjectError::InvalidPath(format!(
                "failed to resolve project path '{}': {err}",
                path.display()
            ))
        })?;
        if !canonical.is_dir() {
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

            if resolved.worktree_root != resolved.main_root {
                let wt_path_str = resolved.worktree_root.to_string_lossy().into_owned();
                let wt_branch = get_current_branch(&resolved.worktree_root);
                let source = self.classify_worktree(&project.name, &resolved.worktree_root);
                let worktree = Worktree::new(wt_path_str, wt_branch, source);
                project.upsert_worktree(worktree.clone());
                tracing::debug!(
                    project = %project.name,
                    worktree = %worktree.path,
                    "resolved linked git worktree under parent project"
                );
                return Ok((project, Some(worktree)));
            }
            return Ok((project, None));
        }

        let root_str = canonical.to_string_lossy().into_owned();
        let project = Project::new(root_str, None, None, Vec::new());
        Ok((project, None))
    }

    /// Refreshes branch labels, reclassifies `WorktreeSource`, and prunes worktrees whose directories no longer exist.
    pub fn refresh_project(&self, project: &mut Project) -> Result<()> {
        let root_path = Path::new(&project.root_path);
        if root_path.is_dir() {
            project.branch = get_current_branch(root_path);
        }
        let project_name = project.name.clone();
        project.worktrees.retain(|wt| Path::new(&wt.path).is_dir());
        for wt in &mut project.worktrees {
            let wt_path = Path::new(&wt.path);
            let branch = get_current_branch(wt_path);
            let source = self.classify_worktree(&project_name, wt_path);
            *wt = Worktree::new(wt.path.clone(), branch, source);
        }
        Ok(())
    }

    /// Computes the managed checkout path (`~/.lgtm/worktrees/<project>/<branch-slug>`) for `branch`.
    pub fn get_worktree_path(&self, project: &Project, branch: &str) -> PathBuf {
        let slug = branch_to_path_slug(branch);
        self.worktrees_root.join(&project.name).join(slug)
    }

    /// Creates a `Managed` Git worktree under `~/.lgtm/worktrees/<project>/<branch-slug>` for `project`.
    pub fn create_worktree(
        &self,
        project: &Project,
        branch: &str,
        base_ref: Option<&str>,
    ) -> Result<Worktree> {
        let root_path = Path::new(&project.root_path);
        if project.git_common_dir.is_none() && !root_path.join(".git").exists() {
            return Err(LgtmProjectError::NotGitRepository(format!(
                "'{}' is not a Git repository.",
                project.name
            )));
        }
        let target_dir = self.get_worktree_path(project, branch);
        if target_dir.exists() {
            return Err(LgtmProjectError::CheckoutPathExists(format!(
                "Worktree path '{}' already exists.",
                target_dir.display()
            )));
        }
        let created =
            add_worktree(root_path, &target_dir, branch, base_ref).map_err(|err| match err {
                RsUtilsError::Invalid(msg) => LgtmProjectError::InvalidBranch(msg),
                RsUtilsError::BranchAlreadyCheckedOut(msg) => {
                    LgtmProjectError::BranchAlreadyCheckedOut(msg)
                }
                other => LgtmProjectError::Git(other.to_string()),
            })?;
        let path_str = created.path.to_string_lossy().into_owned();
        Ok(Worktree::new(
            path_str,
            created.branch,
            WorktreeSource::Managed,
        ))
    }

    /// Deletes disk storage for a `Managed` worktree via `git worktree remove`, or returns [`LgtmProjectError::ExternalWorktree`] for `External` worktrees.
    pub fn remove_worktree(
        &self,
        project: &Project,
        worktree_path: &Path,
        force: bool,
    ) -> Result<()> {
        let canonical =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        let source = project
            .worktrees
            .iter()
            .find(|wt| Path::new(&wt.path) == canonical || Path::new(&wt.path) == worktree_path)
            .map(|wt| wt.source)
            .unwrap_or_else(|| self.classify_worktree(&project.name, &canonical));

        if source == WorktreeSource::External {
            return Err(LgtmProjectError::ExternalWorktree(format!(
                "External worktree '{}' is not managed by LGTM and cannot be deleted from disk.",
                canonical.display()
            )));
        }

        remove_worktree(Path::new(&project.root_path), &canonical, force).map_err(|err| match err {
            RsUtilsError::DirtyWorktree(msg) => LgtmProjectError::DirtyWorktree(msg),
            other => LgtmProjectError::Git(other.to_string()),
        })
    }

    // Private -----------------------------------------------------------------

    fn classify_worktree(&self, project_name: &str, worktree_path: &Path) -> WorktreeSource {
        let managed_base = std::fs::canonicalize(&self.worktrees_root)
            .unwrap_or_else(|_| self.worktrees_root.clone());
        let project_managed_dir = managed_base.join(project_name);
        let candidate =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        if candidate.starts_with(&project_managed_dir) || candidate.starts_with(&managed_base) {
            WorktreeSource::Managed
        } else {
            WorktreeSource::External
        }
    }
}
