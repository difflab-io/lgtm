//! Service orchestrating session persistence (`~/.lgtm/sessions/<id>.json`), project registration, and tab retention.

use std::{
    fs,
    path::{Path, PathBuf},
};

use lgtm_project::{LgtmProjectError, ProjectService, WorktreeSource};
use lgtm_rsutils::atomic_write;
use uuid::Uuid;

use crate::{
    error::{LgtmSessionError, Result},
    models::session::{
        PersistedTab, Session, SidebarWidget, TabCandidate, WorkspaceSessionState,
        WorkspaceStatePatch,
    },
};

/// Coordinates `~/.lgtm/sessions/<id>.json` persistence and [`ProjectService`] worktree lifecycle.
#[derive(Debug, Clone)]
pub struct SessionService {
    sessions_dir: PathBuf,
    projects: ProjectService,
}

impl SessionService {
    /// Creates a session service rooted at `<lgtm_home>/sessions` (or `~/.lgtm/sessions`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(default_lgtm_home);
        Self {
            sessions_dir: base.join("sessions"),
            projects: ProjectService::new(Some(base)),
        }
    }

    // Public ------------------------------------------------------------------

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

        session.active_workspace_root = target_workspace.clone();
        session
            .workspaces
            .entry(target_workspace.clone())
            .or_insert_with(|| WorkspaceSessionState {
                workspace_root: target_workspace,
                last_opened_tab_path: None,
                expanded_dirs: Vec::new(),
                tabs: Vec::new(),
            });

        self.write_session(&session)?;
        Ok(session)
    }

    /// Filters `patch.tabs` to `edited || last_opened`, updates `workspaces[patch.workspace_root]` and optional `sidebar_widget`, and saves the session.
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
            let workspace_root = std::fs::canonicalize(&patch.workspace_root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(patch.workspace_root);
            let retained = PersistedTab::retain_from_candidates(patch.tabs);
            let last_opened_tab_path = retained
                .iter()
                .find(|tab| tab.last_opened)
                .map(|tab| tab.path.clone());
            session.workspaces.insert(
                workspace_root.clone(),
                WorkspaceSessionState {
                    workspace_root,
                    last_opened_tab_path,
                    expanded_dirs: patch.expanded_dirs,
                    tabs: retained,
                },
            );
        }
        self.write_session(&session)?;
        Ok(session)
    }

    /// Convenience helper for updating a workspace's tab candidates and optional `sidebar_widget`.
    pub fn save_workspace_tabs(
        &self,
        session_id: &str,
        workspace_root: &Path,
        tabs: Vec<TabCandidate>,
        expanded_dirs: Vec<String>,
        sidebar_widget: Option<SidebarWidget>,
    ) -> Result<Session> {
        self.save_workspace_state(
            session_id,
            WorkspaceStatePatch {
                workspace_root: workspace_root.to_string_lossy().into_owned(),
                tabs,
                expanded_dirs,
                sidebar_widget,
            },
        )
    }

    /// Collapses or expands child worktrees for `project_id` in `session_id`.
    pub fn set_project_collapsed(
        &self,
        session_id: &str,
        project_id: Uuid,
        collapsed: bool,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let project = session
            .projects
            .iter_mut()
            .find(|p| p.id == project_id)
            .ok_or_else(|| {
                LgtmSessionError::NotFound(format!("Project '{project_id}' not found."))
            })?;
        project.collapsed = collapsed;
        self.write_session(&session)?;
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
        let idx = session
            .projects
            .iter()
            .position(|p| p.id == project_id)
            .ok_or_else(|| {
                LgtmSessionError::NotFound(format!("Project '{project_id}' not found."))
            })?;
        let worktree = self
            .projects
            .create_worktree(&session.projects[idx], branch, base_ref)?;
        let wt_path = worktree.path.clone();
        session.projects[idx].collapsed = false;
        session.projects[idx].upsert_worktree(worktree);
        session.active_workspace_root = wt_path.clone();
        session
            .workspaces
            .entry(wt_path.clone())
            .or_insert_with(|| WorkspaceSessionState {
                workspace_root: wt_path,
                last_opened_tab_path: None,
                expanded_dirs: Vec::new(),
                tabs: Vec::new(),
            });
        self.write_session(&session)?;
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
        let idx = session
            .projects
            .iter()
            .position(|p| p.id == project_id)
            .ok_or_else(|| {
                LgtmSessionError::NotFound(format!("Project '{project_id}' not found."))
            })?;

        let canonical =
            std::fs::canonicalize(worktree_path).unwrap_or_else(|_| worktree_path.to_path_buf());
        let canonical_str = canonical.to_string_lossy().into_owned();
        let raw_str = worktree_path.to_string_lossy().into_owned();

        let source = session.projects[idx]
            .worktrees
            .iter()
            .find(|wt| wt.path == canonical_str || wt.path == raw_str)
            .map(|wt| wt.source)
            .unwrap_or(WorktreeSource::External);

        if source == WorktreeSource::Managed {
            match self
                .projects
                .remove_worktree(&session.projects[idx], &canonical, force)
            {
                Ok(()) | Err(LgtmProjectError::ExternalWorktree(_)) => {}
                Err(err) => return Err(err.into()),
            }
        }

        let parent_root = session.projects[idx].root_path.clone();
        session.projects[idx]
            .worktrees
            .retain(|wt| wt.path != canonical_str && wt.path != raw_str);
        session.workspaces.remove(&canonical_str);
        session.workspaces.remove(&raw_str);

        if session.active_workspace_root == canonical_str
            || session.active_workspace_root == raw_str
        {
            session.active_workspace_root = parent_root;
        }

        self.write_session(&session)?;
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
        Ok(session)
    }

    // Private -----------------------------------------------------------------

    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{session_id}.json"))
    }

    fn write_session(&self, session: &Session) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(session)?;
        atomic_write(&self.session_file_path(&session.id), &bytes)?;
        Ok(())
    }
}

fn normalize_session_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "default".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn default_lgtm_home() -> PathBuf {
    if let Ok(home) = std::env::var("LGTM_HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home.trim());
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lgtm")
}
