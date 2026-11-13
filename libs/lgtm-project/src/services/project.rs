//! Service resolving project roots and managing LGTM-owned Git worktrees.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use lgtm_config::ConfigService;
use lgtm_rsutils::{
    add_worktree, branch_to_path_slug, checkout_branch, get_current_branch, list_local_branches,
    remove_worktree, resolve_git_paths,
};
use uuid::Uuid;

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

    /// Looks up a project by `project_id` in `projects`.
    pub fn get_project<'a>(
        &self,
        projects: &'a [Project],
        project_id: Uuid,
    ) -> Result<&'a Project> {
        projects.iter().find(|p| p.id == project_id).ok_or_else(|| {
            tracing::warn!(%project_id, "project not found");
            LgtmProjectError::NotFound(format!("Project '{project_id}' not found."))
        })
    }

    /// Looks up a mutable project by `project_id` in `projects`.
    pub fn get_project_mut<'a>(
        &self,
        projects: &'a mut [Project],
        project_id: Uuid,
    ) -> Result<&'a mut Project> {
        projects
            .iter_mut()
            .find(|p| p.id == project_id)
            .ok_or_else(|| {
                tracing::warn!(%project_id, "project not found");
                LgtmProjectError::NotFound(format!("Project '{project_id}' not found."))
            })
    }

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
            project.branches = list_local_branches(&resolved.main_root);

            if resolved.worktree_root != resolved.main_root
                && self.is_managed_worktree(&project.name, &resolved.worktree_root)
            {
                let wt_path_str = resolved.worktree_root.to_string_lossy().into_owned();
                let wt_branch = get_current_branch(&resolved.worktree_root);
                let worktree = Worktree::new(wt_path_str, wt_branch);
                project.set_worktree(worktree.clone());
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

    /// Resolves `path`, inserts or updates its parent [`Project`] in `projects`, and returns the activated checkout path.
    pub fn open_project(&self, projects: &mut Vec<Project>, path: &Path) -> Result<String> {
        let (resolved_project, resolved_worktree) = self.resolve_project(path)?;
        let target_workspace = resolved_worktree
            .as_ref()
            .map(|wt| wt.path.clone())
            .unwrap_or_else(|| resolved_project.root_path.clone());

        if let Some(existing) = projects
            .iter_mut()
            .find(|p| p.root_path == resolved_project.root_path || p.id == resolved_project.id)
        {
            existing.branch = resolved_project.branch;
            existing.git_common_dir = resolved_project.git_common_dir;
            existing.branches = resolved_project.branches;
            if let Some(wt) = resolved_worktree {
                existing.set_worktree(wt);
            }
        } else {
            projects.push(resolved_project);
        }
        Ok(target_workspace)
    }

    /// Refreshes branch labels and prunes worktrees whose directories no longer exist while preserving custom worktree section names.
    pub fn refresh_project(&self, project: &mut Project) -> Result<()> {
        let root_path = Path::new(&project.root_path);
        if root_path.is_dir() {
            project.branch = get_current_branch(root_path);
            project.branches = list_local_branches(root_path);
        }
        let dir_name = Project::get_default_name(&project.root_path);
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
        let dir_name = Project::get_default_name(&project.root_path);
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

    /// Switches `checkout_dir` to `branch` (or creates and switches to `-b <branch>` when `create` is `true`).
    pub fn checkout_branch(
        &self,
        checkout_dir: &Path,
        branch: &str,
        create: bool,
    ) -> Result<String> {
        let canonical =
            std::fs::canonicalize(checkout_dir).unwrap_or_else(|_| checkout_dir.to_path_buf());
        checkout_branch(&canonical, branch, create).map_err(|err| {
            tracing::warn!(
                checkout = %canonical.display(),
                branch = %branch,
                create,
                error = %err,
                "failed to checkout git branch"
            );
            match err.kind() {
                ErrorKind::InvalidInput => LgtmProjectError::InvalidBranch(err.to_string()),
                ErrorKind::AlreadyExists => {
                    LgtmProjectError::BranchAlreadyCheckedOut(err.to_string())
                }
                _ => LgtmProjectError::Git(err.to_string()),
            }
        })
    }

    /// Sets the collapsed state for `project_id` in `projects`.
    pub fn set_project_collapsed(
        &self,
        projects: &mut [Project],
        project_id: Uuid,
        collapsed: bool,
    ) -> Result<()> {
        self.get_project_mut(projects, project_id)?
            .set_collapsed(collapsed);
        Ok(())
    }

    /// Updates custom `name`, `icon`, and `color` appearance properties for `project_id` in `projects`.
    pub fn update_project_appearance(
        &self,
        projects: &mut [Project],
        project_id: Uuid,
        name: Option<String>,
        icon: Option<String>,
        color: Option<String>,
    ) -> Result<()> {
        self.get_project_mut(projects, project_id)?
            .update_appearance(name, icon, color);
        Ok(())
    }

    /// Updates the display label of `worktree_path` under `project_id` in `projects`.
    pub fn rename_project_worktree(
        &self,
        projects: &mut [Project],
        project_id: Uuid,
        worktree_path: &Path,
        name: &str,
    ) -> Result<()> {
        let project = self.get_project_mut(projects, project_id)?;
        let canonical_str = std::fs::canonicalize(worktree_path)
            .unwrap_or_else(|_| worktree_path.to_path_buf())
            .to_string_lossy()
            .into_owned();
        let raw_str = worktree_path.to_string_lossy();
        if !project.set_worktree_name(&canonical_str, name)
            && !project.set_worktree_name(&raw_str, name)
        {
            tracing::warn!(%project_id, worktree = %raw_str, "worktree not found for rename");
            return Err(LgtmProjectError::NotFound(format!(
                "Worktree '{}' not found under project '{}'.",
                raw_str, project.name
            )));
        }
        Ok(())
    }

    /// Creates a managed worktree under `project_id` in `projects`, attaches it, and refreshes project branch metadata.
    pub fn create_project_worktree(
        &self,
        projects: &mut [Project],
        project_id: Uuid,
        branch: &str,
        base_ref: Option<&str>,
    ) -> Result<Worktree> {
        let project = self.get_project_mut(projects, project_id)?;
        let worktree = self.create_worktree(project, branch, base_ref)?;
        project.set_collapsed(false);
        project.set_worktree(worktree.clone());
        let _ = self.refresh_project(project);
        Ok(worktree)
    }

    /// Removes `worktree_path` from disk and from `project_id` in `projects`, returning `(canonical_path, raw_path, parent_root_path)`.
    pub fn remove_project_worktree(
        &self,
        projects: &mut [Project],
        project_id: Uuid,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(String, String, String)> {
        let canonical =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        let canonical_str = canonical.to_string_lossy().into_owned();
        let raw_str = worktree_path.to_string_lossy().into_owned();
        let project = self.get_project_mut(projects, project_id)?;
        self.remove_worktree(project, &canonical, force)?;
        project.apply_worktree_removal(&canonical_str, &raw_str);
        let _ = self.refresh_project(project);
        Ok((canonical_str, raw_str, project.root_path.clone()))
    }

    /// Removes and returns `project_id` from `projects`.
    pub fn remove_project(&self, projects: &mut Vec<Project>, project_id: Uuid) -> Result<Project> {
        let idx = projects
            .iter()
            .position(|p| p.id == project_id)
            .ok_or_else(|| {
                tracing::warn!(%project_id, "project not found");
                LgtmProjectError::NotFound(format!("Project '{project_id}' not found."))
            })?;
        Ok(projects.remove(idx))
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
