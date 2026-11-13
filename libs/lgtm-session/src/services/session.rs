//! Service orchestrating session persistence (`~/.lgtm/sessions/<id>.json`), project aggregation, and tab retention.

use std::{
    fs,
    path::{Path, PathBuf},
};

use lgtm_config::ConfigService;
use lgtm_project::ProjectService;
use lgtm_rsutils::atomic_write;
use uuid::Uuid;

use crate::{
    error::Result,
    models::{
        session::Session,
        tab::{PersistedTab, TabCandidate},
        workspace::WorkspaceStatePatch,
    },
};

/// Coordinates `~/.lgtm/sessions/<id>.json` persistence, tab filtering, and [`ProjectService`] delegation.
#[derive(Debug, Clone)]
pub struct SessionService {
    sessions_dir: PathBuf,
    projects: ProjectService,
}

impl SessionService {
    /// Creates a session service rooted at `<lgtm_home>/sessions` (or `~/.lgtm/sessions`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(|| ConfigService::new().get_lgtm_home_dir());
        Self::with_project_service(Some(base.clone()), ProjectService::new(Some(base)))
    }

    /// Creates a session service with an injected [`ProjectService`].
    pub fn with_project_service(lgtm_home: Option<PathBuf>, projects: ProjectService) -> Self {
        let base = lgtm_home.unwrap_or_else(|| ConfigService::new().get_lgtm_home_dir());
        Self {
            sessions_dir: base.join("sessions"),
            projects,
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
        let target_workspace = self.projects.open_project(&mut session.projects, path)?;
        session.set_active_workspace(target_workspace);
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
            session.set_workspace_state(workspace_root, patch.expanded_dirs, retained);
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
        self.projects
            .set_project_collapsed(&mut session.projects, project_id, collapsed)?;
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
        self.projects.update_project_appearance(
            &mut session.projects,
            project_id,
            name,
            icon,
            color,
        )?;
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
        self.projects.rename_project_worktree(
            &mut session.projects,
            project_id,
            worktree_path,
            name,
        )?;
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
        let worktree = self.projects.create_project_worktree(
            &mut session.projects,
            project_id,
            branch,
            base_ref,
        )?;
        session.set_active_workspace(worktree.path);
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, branch = %branch, "created project worktree in session");
        Ok(session)
    }

    /// Switches `workspace_root` to `branch` (or creates `-b <branch>` when `create` is `true`) and refreshes session metadata.
    pub fn checkout_workspace_branch(
        &self,
        session_id: &str,
        workspace_root: &Path,
        branch: &str,
        create: bool,
    ) -> Result<Session> {
        let checked_out = self
            .projects
            .checkout_branch(workspace_root, branch, create)?;
        let session = self.get_session(session_id)?;
        self.write_session(&session)?;
        tracing::info!(
            session_id = %session.id,
            workspace_root = %workspace_root.display(),
            branch = %checked_out,
            create,
            "checked out workspace branch"
        );
        Ok(session)
    }

    /// Removes `worktree_path` from `project_id` and deletes its managed checkout directory on disk.
    pub fn remove_project_worktree(
        &self,
        session_id: &str,
        project_id: Uuid,
        worktree_path: &Path,
        force: bool,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let (canonical_str, raw_str, parent_root) = self.projects.remove_project_worktree(
            &mut session.projects,
            project_id,
            worktree_path,
            force,
        )?;
        session.apply_worktree_removal(&canonical_str, &raw_str, parent_root);
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, "removed project worktree from session");
        Ok(session)
    }

    /// Removes `project_id` and its remembered workspace states from `session_id`.
    pub fn remove_project(&self, session_id: &str, project_id: Uuid) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let removed = self
            .projects
            .remove_project(&mut session.projects, project_id)?;
        session.apply_project_removal(&removed);
        self.write_session(&session)?;
        tracing::info!(session_id = %session.id, %project_id, "removed project from session");
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
