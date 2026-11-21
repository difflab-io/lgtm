//! Session persistence (`SessionService`) IPC command handlers.

use lgtm_session::Session;
use lgtm_workspace::{Result, WorkspaceStatePatch};
use tauri::State;

use crate::{app::AppState, events::AppEvent};

/// Loads the persisted session state (`~/.lgtm/sessions/<id>.json`) with refreshed project branches and worktrees.
#[tauri::command]
#[specta::specta]
pub async fn session_get(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.get_refreshed_session(session_id.as_deref().unwrap_or("default"))
        })
        .await
}

/// Saves the active sidebar widget, expanded explorer folders, and filtered tabs (`edited || last_opened`) for a workspace.
#[tauri::command]
#[specta::specta]
pub async fn session_save(
    state: State<'_, AppState>,
    patch: WorkspaceStatePatch,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            let mut session = app
                .session
                .save_workspace_state("default", patch)
                .map_err(AppState::map_session_error)?;
            for project in &mut session.projects {
                let _ = app.project.refresh_project(project);
            }
            app.events
                .dispatch(AppEvent::SessionUpdated(session.clone()));
            Ok(session)
        })
        .await
}
