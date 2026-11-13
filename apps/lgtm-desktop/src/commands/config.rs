//! Configuration (`ConfigService`) IPC command handlers.

use lgtm_config::ResolvedConfigSnapshot;
use lgtm_workspace::{Result, error};
use tauri::State;

use crate::app::AppState;

/// Returns the resolved nested configuration snapshot across defaults, global settings, and project overrides.
#[tauri::command]
#[specta::specta]
pub async fn config_get(state: State<'_, AppState>) -> Result<ResolvedConfigSnapshot> {
    state
        .run_blocking(move |app| {
            let root = app.lock_workspace()?.get_root().ok();
            app.config
                .get_config_snapshot(root.as_deref())
                .map_err(|err| error("io", err.to_string()))
        })
        .await
}
