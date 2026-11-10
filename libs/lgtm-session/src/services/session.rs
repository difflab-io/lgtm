//! Service orchestrating session persistence (`~/.lgtm/sessions/<id>.json`), project aggregation, and tab retention.

use std::{
    fs,
    path::{Path, PathBuf},
};

use lgtm_config::ConfigService;
use lgtm_project::{LgtmProjectError, Project, ProjectService, WorktreeSource};
use lgtm_rsutils::atomic_write;
use uuid::Uuid;

use crate::{
    error::{LgtmSessionError, Result},
    models::{
        session::Session,
        tab::{PersistedTab, TabCandidate},
        workspace::{WorkspaceSessionState, WorkspaceStatePatch},
    },
};

/// Coordinates `~/.lgtm/sessions/<id>.json` persistence, tab filtering, and [`ProjectService`] aggregation.
#[derive(Debug, Clone)]
pub struct SessionService {
    sessions_dir: PathBuf,
    projects: ProjectService,
}

impl SessionService {
    /// Creates a session service rooted at `<lgtm_home>/sessions` (or `~/.lgtm/sessions`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(|| ConfigService::new().get_lgtm_home_dir());
        Self {
            sessions_dir: base.join("sessions"),
            projects: ProjectService::new(Some(base)),
        }
    }

    // Public ------------------------------------------------------------------

    /// Filters `candidates` so only tabs where [`TabCandidate::should_retain`] is `true` (`edited || last_opened`) are persisted.
    pub fn retain_tabs(candidates: Vec<TabCandidate>) -> Vec<PersistedTab> {
        candidates
            .into_iter()
            .filter(TabCandidate::should_retain)
            .map(PersistedTab::from)
            .collect()
    }

    /// Loads `~/.lgtm/sessions/<session_id>.json` (creating an empty session if none exists yet) and refreshes project branches.
    pub fn get_session(&self, session_id: &str) -> Result<Session> {
        let id = normalize_session_id(session_id);
        let path = self.session_file_path(&id);
        let mut session = if path.is_file() {
            let bytes = fs::read(&path)?;
            serde_json::from_slice::<Session>(&bytes)?
        } else {
            Session::new(&id)
        };

        for project in &mut session.projects {
            let _ = self.projects.refresh_project(project);
        }
        tracing::debug!(
            session_id = %session.id,
            projects = session.projects.len(),
            active_workspace = %session.active_workspace_root,
            "loaded session state"
        );
        Ok(session)
    }

    /// Resolves `path` into its parent `Project` and optional `Worktree`, registers it in `session_id`, sets `active_workspace_root`, and saves the session.
    pub fn open_workspace(&self, session_id: &str, path: &Path) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let (resolved_project, resolved_worktree) = self.projects.resolve_project(path)?;
        let target_workspace = resolved_worktree
            .as_ref()
            .map(|wt| wt.path.clone())
            .unwrap_or_else(|| resolved_project.root_path.clone());

        if let Some(existing) = session
            .projects
            .iter_mut()
            .find(|p| p.root_path == resolved_project.root_path || p.id == resolved_project.id)
        {
            existing.branch = resolved_project.branch;
            existing.git_common_dir = resolved_project.git_common_dir;
            if let Some(wt) = resolved_worktree {
                existing.upsert_worktree(wt);
            }
        } else {
            session.projects.push(resolved_project);
        }

        self.activate_workspace(&mut session, target_workspace);
        self.write_session(&session)?;
        tracing::info!(
            session_id = %session.id,
            active_workspace = %session.active_workspace_root,
            "opened workspace in session"
        );
        Ok(session)
    }

    /// Filters `patch.tabs` via [`Self::retain_tabs`], updates `workspaces[patch.workspace_root]` and optional `sidebar_widget`, and saves the session.
    pub fn save_workspace_state(
        &self,
        session_id: &str,
        patch: WorkspaceStatePatch,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        if let Some(widget) = patch.sidebar_widget {
            session.sidebar_widget = widget;
        }
        if !patch.workspace_root.trim().is_empty() {
            let workspace_root = canonicalize_path_str(Path::new(&patch.workspace_root));
            let retained = Self::retain_tabs(patch.tabs);
            tracing::info!(
                session_id = %session.id,
                workspace_root = %workspace_root,
                retained_tabs = retained.len(),
                "saved workspace session state"
            );
            session.workspaces.insert(
                workspace_root.clone(),
                WorkspaceSessionState::new(workspace_root, patch.expanded_dirs, retained),
            );
        }
        self.write_session(&session)?;
        Ok(session)
    }

    /// Collapses or expands child worktrees for `project_id` in `session_id`.
    pub fn set_project_collapsed(
        &self,
        session_id: &str,
        project_id: Uuid,
        collapsed: bool,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        find_project_mut(&mut session, project_id)?.collapsed = collapsed;
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, collapsed, "updated project collapsed state");
        Ok(session)
    }

    /// Updates custom `name`, `icon`, and `color` metadata for `project_id` in `session_id` and saves the session.
    pub fn update_project_appearance(
        &self,
        session_id: &str,
        project_id: Uuid,
        name: Option<String>,
        icon: Option<String>,
        color: Option<String>,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        find_project_mut(&mut session, project_id)?.update_appearance(name, icon, color);
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, "updated project appearance");
        Ok(session)
    }

    /// Renames the display label of `worktree_path` under `project_id` without changing its Git branch name.
    pub fn rename_project_worktree(
        &self,
        session_id: &str,
        project_id: Uuid,
        worktree_path: &Path,
        name: &str,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let project = find_project_mut(&mut session, project_id)?;
        let canonical_str = canonicalize_path_str(worktree_path);
        let raw_str = worktree_path.to_string_lossy();
        if !project.rename_worktree(&canonical_str, name)
            && !project.rename_worktree(&raw_str, name)
        {
            tracing::warn!(%project_id, worktree = %raw_str, "worktree not found for rename");
            return Err(LgtmSessionError::NotFound(format!(
                "Worktree '{}' not found under project '{}'.",
                raw_str, project.name
            )));
        }
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, name = %name, "renamed project worktree");
        Ok(session)
    }

    /// Creates a `Managed` worktree under `~/.lgtm/worktrees/<project>/<branch-slug>`, attaches it to `project_id`, activates it, and saves the session.
    pub fn create_project_worktree(
        &self,
        session_id: &str,
        project_id: Uuid,
        branch: &str,
        base_ref: Option<&str>,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let project = find_project_mut(&mut session, project_id)?;
        let worktree = self.projects.create_worktree(project, branch, base_ref)?;
        let wt_path = worktree.path.clone();
        project.collapsed = false;
        project.upsert_worktree(worktree);
        self.activate_workspace(&mut session, wt_path);
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, branch = %branch, "created project worktree in session");
        Ok(session)
    }

    /// Removes `worktree_path` from `project_id`, deleting disk storage when `WorktreeSource::Managed` and detaching without disk deletion when `External`.
    pub fn remove_project_worktree(
        &self,
        session_id: &str,
        project_id: Uuid,
        worktree_path: &Path,
        force: bool,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let canonical =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        let canonical_str = canonical.to_string_lossy().into_owned();
        let raw_str = worktree_path.to_string_lossy().into_owned();

        let (parent_root, source) = {
            let project = find_project_mut(&mut session, project_id)?;
            let source = project
                .worktrees
                .iter()
                .find(|wt| wt.path == canonical_str || wt.path == raw_str)
                .map(|wt| wt.source)
                .unwrap_or(WorktreeSource::External);
            if source == WorktreeSource::Managed {
                match self.projects.remove_worktree(project, &canonical, force) {
                    Ok(()) | Err(LgtmProjectError::ExternalWorktree(_)) => {}
                    Err(err) => return Err(err.into()),
                }
            }
            project
                .worktrees
                .retain(|wt| wt.path != canonical_str && wt.path != raw_str);
            (project.root_path.clone(), source)
        };

        session.workspaces.remove(&canonical_str);
        session.workspaces.remove(&raw_str);
        if session.active_workspace_root == canonical_str
            || session.active_workspace_root == raw_str
        {
            session.active_workspace_root = parent_root;
        }

        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, source = ?source, "removed project worktree from session");
        Ok(session)
    }

    /// Removes `project_id` and its remembered workspace states from `session_id`.
    pub fn remove_project(&self, session_id: &str, project_id: Uuid) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let idx = session
            .projects
            .iter()
            .position(|p| p.id == project_id)
            .ok_or_else(|| {
                tracing::warn!(%project_id, "project not found in session");
                LgtmSessionError::NotFound(format!("Project '{project_id}' not found."))
            })?;
        let removed = session.projects.remove(idx);
        session.workspaces.remove(&removed.root_path);
        for wt in &removed.worktrees {
            session.workspaces.remove(&wt.path);
        }
        if removed.contains_workspace(&session.active_workspace_root) {
            session.active_workspace_root = session
                .projects
                .first()
                .map(|p| p.root_path.clone())
                .unwrap_or_default();
        }
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, "removed project from session");
        Ok(session)
    }

    // Private -----------------------------------------------------------------

    fn activate_workspace(&self, session: &mut Session, workspace_root: String) {
        session.active_workspace_root = workspace_root.clone();
        session
            .workspaces
            .entry(workspace_root.clone())
            .or_insert_with(|| WorkspaceSessionState::new(workspace_root, Vec::new(), Vec::new()));
    }

    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{session_id}.json"))
    }

    fn write_session(&self, session: &Session) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(session)?;
        atomic_write(&self.session_file_path(&session.id), &bytes)?;
        Ok(())
    }
}

fn find_project_mut(session: &mut Session, project_id: Uuid) -> Result<&mut Project> {
    session
        .projects
        .iter_mut()
        .find(|p| p.id == project_id)
        .ok_or_else(|| {
            tracing::warn!(%project_id, "project not found in session");
            LgtmSessionError::NotFound(format!("Project '{project_id}' not found."))
        })
}

fn canonicalize_path_str(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn normalize_session_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "default".to_owned()
    } else {
        trimmed.to_owned()
    }
}
