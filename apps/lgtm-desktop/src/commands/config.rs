//! Configuration (`ConfigService`) IPC command handlers.

use lgtm_config::ResolvedConfigSnapshot;
use lgtm_workspace::{Result, error};
use tauri::State;

use crate::{app::AppState, commands::run_workspace_blocking};

/// Returns the resolved nested configuration snapshot across defaults, global settings, and project overrides.
#[tauri::command]
#[specta::specta]
pub async fn config_get(state: State<'_, AppState>) -> Result<ResolvedConfigSnapshot> {
    let config_service = state.config.clone();
    run_workspace_blocking(state, move |w| {
        let root = w.get_root().ok();
        config_service
            .get_config_snapshot(root.as_deref())
            .map_err(|err| error("io", err.to_string()))
    })
    .await
}
