//! Language Server Protocol (`LspService`) IPC command handlers.

use lgtm_lsp::{
    LspDiagnosticsBuffer, LspHoverResult, LspLocation, LspServerStatusEvent, LspWorkspaceSnapshot,
};
use lgtm_workspace::Result;
use tauri::State;

use crate::app::AppState;

/// Synchronizes in-memory editor changes (`textDocument/didChange`) with the language server.
#[tauri::command]
#[specta::specta]
pub async fn lsp_did_change(
    state: State<'_, AppState>,
    relative_path: String,
    content: String,
) -> Result<()> {
    state
        .run_blocking(move |app| {
            let (root, file_path) = {
                let w = app.lock_workspace()?;
                (w.get_root()?, w.get_entry_path(&relative_path)?)
            };
            app.lsp.did_change(&root, &file_path, &content)
        })
        .await
}

/// Requests hover documentation (`textDocument/hover`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_hover(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Option<LspHoverResult>> {
    state
        .run_blocking(move |app| {
            let (root, file_path) = {
                let w = app.lock_workspace()?;
                (w.get_root()?, w.get_entry_path(&relative_path)?)
            };
            app.lsp.get_hover(&root, &file_path, offset)
        })
        .await
}

/// Requests symbol definition locations (`textDocument/definition`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_goto_definition(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Vec<LspLocation>> {
    state
        .run_blocking(move |app| {
            let (root, file_path) = {
                let w = app.lock_workspace()?;
                (w.get_root()?, w.get_entry_path(&relative_path)?)
            };
            app.lsp.get_definition(&root, &file_path, offset)
        })
        .await
}

/// Returns the current workspace LSP server statuses and diagnostics snapshot.
#[tauri::command]
#[specta::specta]
pub async fn lsp_get_state(state: State<'_, AppState>) -> Result<LspWorkspaceSnapshot> {
    state
        .run_blocking(move |app| {
            let root = app.lock_workspace()?.get_root()?;
            app.lsp.get_workspace_snapshot(&root)
        })
        .await
}

/// Renders a read-only diagnostics buffer for the active workspace, optionally filtered by `language_id`.
#[tauri::command]
#[specta::specta]
pub async fn lsp_get_diagnostics_buffer(
    state: State<'_, AppState>,
    language_id: Option<String>,
) -> Result<LspDiagnosticsBuffer> {
    state
        .run_blocking(move |app| {
            let root = app.lock_workspace()?.get_root()?;
            app.lsp
                .get_diagnostics_buffer(&root, language_id.as_deref())
        })
        .await
}

/// Restarts the language server for `language_id` in the active workspace.
#[tauri::command]
#[specta::specta]
pub async fn lsp_restart(
    state: State<'_, AppState>,
    language_id: String,
) -> Result<LspServerStatusEvent> {
    state
        .run_blocking(move |app| {
            let root = app.lock_workspace()?.get_root()?;
            app.lsp.restart_language_server(&root, &language_id)
        })
        .await
}
