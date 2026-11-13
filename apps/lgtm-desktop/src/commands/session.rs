//! Session persistence (`SessionService`) IPC command handlers.

use lgtm_session::{Session, WorkspaceStatePatch};
use lgtm_workspace::Result;
use tauri::State;

use crate::app::AppState;

/// Loads the persisted session state (`~/.lgtm/sessions/<id>.json`).
#[tauri::command]
#[specta::specta]
pub async fn session_get(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            Ok(app
                .session
                .get_session(session_id.as_deref().unwrap_or("default"))?)
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
            app.mutate_session(move |s| Ok(s.save_workspace_state("default", patch)?))
        })
        .await
}
