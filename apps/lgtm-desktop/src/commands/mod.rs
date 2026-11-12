//! Tauri IPC command modules grouped by domain (`workspace`, `lsp`, `config`, `session`, and `project`).

pub mod config;
pub mod lsp;
pub mod project;
pub mod session;
pub mod workspace;

pub use config::*;
pub use lsp::*;
pub use project::*;
pub use session::*;
pub use workspace::*;

use lgtm_session::{Session, SessionService};
use lgtm_workspace::{Result, WorkspaceService, error};
use tauri::State;
use uuid::Uuid;

use crate::{app::AppState, events::AppEvent, services::LspService};

/// Runs a blocking `WorkspaceService` operation on Tauri's blocking thread pool while holding the workspace mutex.
pub(crate) async fn run_workspace_blocking<T: Send + 'static>(
    state: State<'_, AppState>,
    operation: impl FnOnce(&mut WorkspaceService) -> Result<T> + Send + 'static,
) -> Result<T> {
    let workspace = state.workspace.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut locked = workspace
            .lock()
            .map_err(|_| error("io", "Workspace service is unavailable."))?;
        operation(&mut locked)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Runs a blocking `LspService` operation on Tauri's blocking thread pool.
pub(crate) async fn run_lsp_blocking<T: Send + 'static>(
    state: State<'_, AppState>,
    operation: impl FnOnce(&LspService) -> Result<T> + Send + 'static,
) -> Result<T> {
    let lsp_service = state.lsp.clone();
    tauri::async_runtime::spawn_blocking(move || operation(&lsp_service))
        .await
        .map_err(|e| error("io", e.to_string()))?
}

/// Runs a read-only `SessionService` operation on Tauri's blocking thread pool.
pub(crate) async fn run_session_blocking<T: Send + 'static>(
    state: State<'_, AppState>,
    operation: impl FnOnce(&SessionService) -> Result<T> + Send + 'static,
) -> Result<T> {
    let session_service = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || operation(&session_service))
        .await
        .map_err(|e| error("io", e.to_string()))?
}

/// Runs a mutating `SessionService` operation on Tauri's blocking thread pool and broadcasts `AppEvent::SessionUpdated`.
pub(crate) async fn run_session_mut_blocking(
    state: State<'_, AppState>,
    operation: impl FnOnce(&SessionService) -> Result<Session> + Send + 'static,
) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = operation(&session_service)?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Parses and validates a project `Uuid` string for IPC command handlers.
pub(crate) fn parse_project_id(project_id: &str) -> Result<Uuid> {
    Uuid::parse_str(project_id.trim()).map_err(|err| error("invalidProject", err.to_string()))
}
