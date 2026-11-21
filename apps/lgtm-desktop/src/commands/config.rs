//! Configuration (`ConfigService`) IPC command handlers.

use lgtm_config::{ConfigScope, LgtmConfigPatch, ResolvedConfigSnapshot};
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

/// Applies a partial configuration patch to global or project `config.json` and emits `config-updated`.
#[tauri::command]
#[specta::specta]
pub async fn config_update(
    state: State<'_, AppState>,
    patch: LgtmConfigPatch,
    scope: Option<ConfigScope>,
) -> Result<ResolvedConfigSnapshot> {
    state
        .run_blocking(move |app| {
            let root = app.lock_workspace()?.get_root().ok();
            let snapshot = app
                .config
                .update_config(root.as_deref(), patch, scope)
                .map_err(|err| error("io", err.to_string()))?;
            if let Some(root) = root.as_deref() {
                app.lsp.reload_workspace_config(root);
            }
            app.events.emit_to_webview("config-updated", &snapshot);
            Ok(snapshot)
        })
        .await
}
