//! Service managing atomic session persistence (`~/.lgtm/sessions/<id>.json`) and workspace session state updates.

use std::{fs, path::PathBuf};

use lgtm_config::ConfigService;
use lgtm_rsutils::atomic_write;
use lgtm_workspace::{WorkspaceService, WorkspaceStatePatch};

use crate::{error::Result, models::session::Session};

/// Persists and restores `~/.lgtm/sessions/<id>.json` snapshots without wrapping sibling domain services.
#[derive(Debug, Clone)]
pub struct SessionService {
    sessions_dir: PathBuf,
}

impl SessionService {
    /// Creates a session service rooted at `<lgtm_home>/sessions` (or `~/.lgtm/sessions`).
    pub fn new(lgtm_home: Option<PathBuf>) -> Self {
        let base = lgtm_home.unwrap_or_else(|| ConfigService::new().get_lgtm_home_dir());
        Self {
            sessions_dir: base.join("sessions"),
        }
    }

    // Public ------------------------------------------------------------------

    /// Loads `~/.lgtm/sessions/<session_id>.json` or initializes a new empty session if none exists yet.
    pub fn get_session(&self, session_id: &str) -> Result<Session> {
        let id = normalize_session_id(session_id);
        let path = self.session_file_path(&id);
        let session = if path.is_file() {
            let bytes = fs::read(&path)?;
            serde_json::from_slice::<Session>(&bytes)?
        } else {
            Session::new(&id)
        };

        tracing::debug!(
            session_id = %session.id,
            projects = session.projects.len(),
            active_workspace = %session.active_workspace_root,
            "loaded session state"
        );
        Ok(session)
    }

    /// Atomically persists `session` to `~/.lgtm/sessions/<session.id>.json`.
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(session)?;
        atomic_write(&self.session_file_path(&session.id), &bytes)?;
        tracing::debug!(
            session_id = %session.id,
            projects = session.projects.len(),
            active_workspace = %session.active_workspace_root,
            "saved session state"
        );
        Ok(())
    }

    /// Delegates widget and workspace tab filtering to [`WorkspaceService::prepare_session_state`] and saves the session.
    pub fn save_workspace_state(
        &self,
        session_id: &str,
        patch: WorkspaceStatePatch,
    ) -> Result<Session> {
        let mut session = self.get_session(session_id)?;
        let right_dock_open = patch.right_dock_open;
        let right_dock_widget = patch.right_dock_widget;
        let right_dock_width = patch.right_dock_width;
        let agents_pane_height = patch.agents_pane_height;
        let (sidebar_widget, workspace_state) = WorkspaceService::prepare_session_state(patch);
        if let Some(widget) = sidebar_widget {
            session.sidebar_widget = widget;
        }
        if let Some(open) = right_dock_open {
            session.right_dock_open = open;
        }
        if let Some(widget) = right_dock_widget {
            session.right_dock_widget = widget;
        }
        if let Some(width) = right_dock_width {
            session.right_dock_width = Some(width);
        }
        if let Some(height) = agents_pane_height {
            session.agents_pane_height = Some(height);
        }
        if let Some(state) = workspace_state {
            tracing::info!(
                session_id = %session.id,
                workspace_root = %state.workspace_root,
                retained_tabs = state.tabs.len(),
                "saved workspace session state"
            );
            session.set_workspace_state(state);
        }
        self.save_session(&session)?;
        Ok(session)
    }

    // Private -----------------------------------------------------------------

    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{session_id}.json"))
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
