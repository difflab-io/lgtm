//! Session persistence (`SessionService`) IPC command handlers.

use lgtm_session::{Session, WorkspaceStatePatch};
use lgtm_workspace::Result;
use tauri::State;

use crate::{
    app::AppState,
    commands::{run_session_blocking, run_session_mut_blocking},
};

/// Loads the persisted session state (`~/.lgtm/sessions/<id>.json`).
#[tauri::command]
#[specta::specta]
pub async fn session_get(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Session> {
    run_session_blocking(state, move |s| {
        Ok(s.get_session(session_id.as_deref().unwrap_or("default"))?)
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
    run_session_mut_blocking(
        state,
        move |s| Ok(s.save_workspace_state("default", patch)?),
    )
    .await
}
