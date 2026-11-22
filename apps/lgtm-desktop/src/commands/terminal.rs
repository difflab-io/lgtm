//! Thin Tauri IPC command handlers for persistent `herdr-core` terminal sessions and Channel streaming.

use std::path::PathBuf;
use std::sync::Arc;

use lgtm_terminal::{TerminalSession, TerminalStreamEvent};
use lgtm_workspace::Result;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::app::AppState;

/// Envelope emitted on `"terminal-stream"` when attaching without a Tauri `Channel` (such as headless E2E IPC bridge).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalStreamPush {
    /// Target `herdr-core` terminal session ID.
    pub session_id: String,
    /// Streamed replay, output, session update, or exit event.
    pub event: TerminalStreamEvent,
}

/// Lists live `herdr-core` terminal sessions, optionally filtered to `workspace_root`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_list(
    state: State<'_, AppState>,
    workspace_root: Option<String>,
) -> Result<Vec<TerminalSession>> {
    state
        .run_blocking(move |app| {
            let root = workspace_root.map(PathBuf::from);
            app.terminal
                .get_sessions(root.as_deref())
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Spawns a new persistent shell session inside `workspace_root`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_create(
    state: State<'_, AppState>,
    workspace_root: String,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession> {
    state
        .run_blocking(move |app| {
            app.terminal
                .create_session(&PathBuf::from(workspace_root), cols, rows)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Attaches a live Tauri `Channel` push stream to `session_id`.
#[tauri::command]
pub async fn terminal_attach(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
    on_event: tauri::ipc::Channel<TerminalStreamEvent>,
) -> Result<TerminalSession> {
    state
        .run_blocking(move |app| {
            let sink: lgtm_terminal::TerminalStreamCallback = Arc::new(move |evt| {
                let _ = on_event.send(evt);
            });
            app.terminal
                .attach(&session_id, cols, rows, sink)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Attaches a live push stream to `session_id` forwarded via `"terminal-stream"` webview events (used by headless E2E IPC bridge).
#[tauri::command]
#[specta::specta]
pub async fn terminal_attach_events(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession> {
    state
        .run_blocking(move |app| {
            let events = app.events.clone();
            let sid = session_id.clone();
            let sink: lgtm_terminal::TerminalStreamCallback = Arc::new(move |evt| {
                events.emit_to_webview(
                    "terminal-stream",
                    &TerminalStreamPush {
                        session_id: sid.clone(),
                        event: evt,
                    },
                );
            });
            app.terminal
                .attach(&session_id, cols, rows, sink)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Detaches the active UI stream from `session_id` while leaving the `herdr-core` session running in the background.
#[tauri::command]
#[specta::specta]
pub async fn terminal_detach(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state
        .run_blocking(move |app| {
            app.terminal
                .detach(&session_id)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Marks any unseen `Done` completion on `session_id` as seen, clearing its blue status dot back to green.
#[tauri::command]
#[specta::specta]
pub async fn terminal_mark_seen(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<TerminalSession>> {
    state
        .run_blocking(move |app| {
            app.terminal
                .mark_seen(&session_id)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Writes raw keyboard or paste bytes to the currently attached `session_id`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_write(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<()> {
    state
        .run_blocking(move |app| {
            app.terminal
                .write_input(&session_id, &data)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Resizes the currently attached `session_id`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    state
        .run_blocking(move |app| {
            app.terminal
                .resize(&session_id, cols, rows)
                .map_err(AppState::map_terminal_error)
        })
        .await
}

/// Closes and terminates `session_id` inside `herdr-core`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_close(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state
        .run_blocking(move |app| {
            app.terminal
                .close_session(&session_id)
                .map_err(AppState::map_terminal_error)
        })
        .await
}
