//! Tauri application state, lifecycle, Specta TypeScript binding generation, and IPC command handlers.
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use lgtm_config::{ConfigScope, ConfigService, LgtmConfigPatch, ResolvedConfigSnapshot};
use lgtm_lsp::{
    LspCompletionItem, LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation,
    LspServerStatusEvent, LspSignatureHelp, LspWorkspaceSnapshot,
};
use lgtm_session::{Session, SessionService, WorkspaceStatePatch};
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{
    error::{Result, error},
    events::{AppEvent, AppEvents, FileOpenedEvent},
    menus::{build_app_menu, handle_menu_event},
    models::{FileContent, WorkspaceListing, WorkspaceNode},
    services::{LspService, WorkspaceService},
};

/// Combined desktop host state managed by Tauri across all IPC command handlers.
#[derive(Clone)]
pub struct AppState {
    /// Shared workspace filesystem service guarded across file and config operations.
    pub workspace: Arc<Mutex<WorkspaceService>>,
    /// Shared session and project persistence service.
    pub session: SessionService,
    /// Shared configuration service resolving global and workspace settings.
    pub config: ConfigService,
    /// Shared desktop LSP orchestration service.
    pub lsp: Arc<LspService>,
    /// In-process backend event bus and webview notification publisher.
    pub events: AppEvents,
    /// Tracks whether the frontend has registered its quit lifecycle listener.
    pub frontend_ready: Arc<AtomicBool>,
}

/// Construct the app; an explicit home is used by the isolated desktop smoke harness.
pub fn builder(workspace_home: Option<PathBuf>) -> tauri::Builder<tauri::Wry> {
    configure_builder(
        tauri::Builder::default()
            .menu(build_app_menu)
            .on_menu_event(|app, event| {
                if let Some(state) = app.try_state::<AppState>() {
                    handle_menu_event(&state.events, event.id().as_ref());
                }
            }),
        workspace_home,
    )
}

/// Configure any Tauri runtime builder with real workspace, config, and LSP services and IPC handlers.
pub fn configure_builder<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    workspace_home: Option<PathBuf>,
) -> tauri::Builder<R> {
    builder
        .setup(move |app| {
            init_app_state(app.handle(), workspace_home)?;
            Ok(())
        })
        .invoke_handler(commands)
}

pub(crate) fn init_app_state<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    workspace_home: Option<PathBuf>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if app_handle.try_state::<AppState>().is_some() {
        return Ok(());
    }
    let home = match workspace_home {
        Some(home) => home,
        None => app_handle.path().home_dir()?,
    };
    let config = ConfigService::from_home(home.clone());
    let session = SessionService::new(Some(home.join(".lgtm")));
    let events = AppEvents::from_app_handle(app_handle);
    let lsp_events = events.clone();
    let lsp = Arc::new(LspService::new(config.clone(), move |event| match event {
        LspEvent::Status(payload) => lsp_events.dispatch(AppEvent::LspStatus(payload)),
        LspEvent::Diagnostics(payload) => lsp_events.dispatch(AppEvent::LspDiagnostics(payload)),
    }));
    let handler_events = events.clone();
    let handler_lsp = lsp.clone();
    let handler_config = config.clone();
    events.set_handler(move |event| {
        handle_app_event(&handler_events, &handler_lsp, &handler_config, event);
    });
    let workspace_path = ConfigService::resolve_desktop_workspace();
    app_handle.manage(AppState {
        workspace: Arc::new(Mutex::new(WorkspaceService::new(home, workspace_path))),
        session,
        config,
        lsp,
        events,
        frontend_ready: Arc::new(AtomicBool::new(false)),
    });
    Ok(())
}

fn handle_app_event(events: &AppEvents, lsp: &LspService, config: &ConfigService, event: AppEvent) {
    match event {
        AppEvent::WorkspaceOpened {
            previous_root,
            next_root,
        } => {
            lsp.switch_workspace(previous_root.as_deref(), &next_root);
        }
        AppEvent::FileOpened(payload) => {
            events.emit_to_webview("workspace-open-file", &payload);
        }
        AppEvent::FileSaved {
            root,
            path,
            content,
            is_config,
        } => {
            let _ = lsp.did_save(&root, &path, &content);
            if is_config && let Ok(snapshot) = config.get_config_snapshot(Some(&root)) {
                lsp.reload_workspace_config(&root);
                events.emit_to_webview("config-updated", &snapshot);
            }
        }
        AppEvent::FileClosed { root, path } => {
            let _ = lsp.did_close(&root, &path);
        }
        AppEvent::LspStatus(payload) => {
            events.emit_to_webview("lsp-status", &payload);
        }
        AppEvent::LspDiagnostics(payload) => {
            events.emit_to_webview("lsp-diagnostics", &payload);
        }
        AppEvent::SessionUpdated(payload) => {
            events.emit_to_webview("session-updated", &payload);
        }
        AppEvent::QuitRequested => {
            events.emit_to_webview("workspace-quit-requested", &());
        }
    }
}

/// Shared command dispatch for the app and the native integration harness.
pub fn commands<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        workspace_open,
        workspace_pick_folder,
        workspace_list,
        workspace_read,
        workspace_write,
        workspace_create_file,
        workspace_create_folder,
        workspace_rename,
        workspace_delete,
        workspace_frontend_ready,
        workspace_finish_quit,
        lsp_did_change,
        lsp_hover,
        lsp_goto_definition,
        lsp_completion,
        lsp_signature_help,
        lsp_get_state,
        lsp_get_diagnostics_buffer,
        lsp_restart,
        config_get,
        session_get,
        session_save,
        project_set_collapsed,
        project_create_worktree,
        project_remove_worktree,
        project_remove,
    ];
    handler(invoke)
}

/// Builds the `tauri-specta` command and type registry for exporting TypeScript bindings.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .disable_serde_phases()
        .dangerously_cast_bigints_to_number()
        .typ::<FileOpenedEvent>()
        .commands(tauri_specta::collect_commands![
            workspace_open,
            workspace_pick_folder,
            workspace_list,
            workspace_read,
            workspace_write,
            workspace_create_file,
            workspace_create_folder,
            workspace_rename,
            workspace_delete,
            workspace_frontend_ready,
            lsp_did_change,
            lsp_hover,
            lsp_goto_definition,
            lsp_completion,
            lsp_signature_help,
            lsp_get_state,
            lsp_get_diagnostics_buffer,
            lsp_restart,
            config_get,
            session_get,
            session_save,
            project_set_collapsed,
            project_create_worktree,
            project_remove_worktree,
            project_remove,
        ])
}

/// Exports the TypeScript bindings file (`apps/lgtm-web/src/lib/gen/bindings.ts`) from the Specta registry.
pub fn export_typescript_bindings_to(
    path: &std::path::Path,
) -> std::result::Result<String, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let exporter =
        specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */\n");
    specta_builder()
        .export(exporter, path)
        .map_err(|err| err.to_string())?;
    std::fs::read_to_string(path).map_err(|err| err.to_string())
}

pub(crate) fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// User-requested quit coordinates with the frontend before exiting.
pub fn handle_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested {
        code: None, api, ..
    } = event
        && let Some(state) = app.try_state::<AppState>()
        && state.frontend_ready.load(Ordering::Relaxed)
        && !app.webview_windows().is_empty()
    {
        api.prevent_exit();
        state.events.dispatch(AppEvent::QuitRequested);
    }
}

/// Builds and runs the Tauri desktop application.
pub fn run() {
    builder(None)
        .build(app_context())
        .expect("error while building Tauri application")
        .run(handle_event);
}

async fn run_blocking<T: Send + 'static>(
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

async fn run_lsp_blocking<T: Send + 'static>(
    state: State<'_, AppState>,
    operation: impl FnOnce(&LspService) -> Result<T> + Send + 'static,
) -> Result<T> {
    let lsp_service = state.lsp.clone();
    tauri::async_runtime::spawn_blocking(move || operation(&lsp_service))
        .await
        .map_err(|e| error("io", e.to_string()))?
}

/// Opens the active or specified workspace directory, updates the `default` session, and prepares workspace language servers.
#[tauri::command]
#[specta::specta]
pub async fn workspace_open(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<WorkspaceListing> {
    let config_service = state.config.clone();
    let session_service = state.session.clone();
    let events = state.events.clone();
    run_blocking(state, move |w| {
        let previous_root = w.get_root().ok();
        let effective_path = match path {
            Some(explicit) => Some(explicit),
            None if !w.is_custom_root() => session_service
                .get_session("default")
                .ok()
                .map(|s| s.active_workspace_root)
                .filter(|active| !active.trim().is_empty() && std::path::Path::new(active).is_dir())
                .or_else(|| {
                    config_service
                        .get_config_snapshot(None)
                        .ok()
                        .and_then(|snap| snap.effective.workspace.active_root)
                        .filter(|active| {
                            !active.trim().is_empty() && std::path::Path::new(active).is_dir()
                        })
                }),
            None => None,
        };
        let listing = w.open(effective_path.as_deref())?;
        if let Ok(next_root) = w.get_root() {
            if let Ok(session) = session_service.open_workspace("default", &next_root) {
                events.dispatch(AppEvent::SessionUpdated(session));
            }
            events.dispatch(AppEvent::WorkspaceOpened {
                previous_root,
                next_root,
            });
        }
        Ok(listing)
    })
    .await
}

/// Opens a native OS folder picker dialog.
#[tauri::command]
#[specta::specta]
pub async fn workspace_pick_folder() -> Result<Option<String>> {
    Ok(rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|folder| folder.path().to_string_lossy().into_owned()))
}

/// Lists visible entries inside a workspace-relative directory.
#[tauri::command]
#[specta::specta]
pub async fn workspace_list(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<Vec<WorkspaceNode>> {
    run_blocking(state, move |w| w.get_entries(&relative_path)).await
}

/// Reads UTF-8 file contents and synchronizes the document with its language server.
#[tauri::command]
#[specta::specta]
pub async fn workspace_read(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<FileContent> {
    let lsp_service = state.lsp.clone();
    let (mut file, root_and_path) = run_blocking(state, move |w| {
        let file = w.get_file_content(&relative_path)?;
        let root_and_path = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        Ok((file, root_and_path))
    })
    .await?;
    if let Some((root, path)) = root_and_path {
        let (language_id, lsp_status) = lsp_service.open_or_sync_file(&root, &path, &file.content);
        file.language_id = language_id;
        file.lsp_status = lsp_status;
    }
    Ok(file)
}

/// Writes UTF-8 file contents to disk and dispatches `AppEvent::FileSaved`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_write(
    state: State<'_, AppState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    let events = state.events.clone();
    let saved_content = content.clone();
    let (saved, save_context) = run_blocking(state, move |w| {
        let saved = w.write_file(&relative_path, &content)?;
        let context = w
            .get_root()
            .ok()
            .zip(w.get_entry_path(&relative_path).ok())
            .map(|(root, path)| {
                let is_config = w.is_config_path(&path);
                (root, path, is_config)
            });
        Ok((saved, context))
    })
    .await?;
    if let Some((root, path, is_config)) = save_context {
        events.dispatch(AppEvent::FileSaved {
            root,
            path,
            content: saved_content,
            is_config,
        });
    }
    Ok(saved)
}

/// Creates a new file inside a workspace-relative directory.
#[tauri::command]
#[specta::specta]
pub async fn workspace_create_file(
    state: State<'_, AppState>,
    name: String,
    parent_path: Option<String>,
) -> Result<WorkspaceNode> {
    run_blocking(state, move |w| {
        w.create_file(&name, parent_path.as_deref().unwrap_or(""))
    })
    .await
}

/// Creates a new folder inside a workspace-relative directory.
#[tauri::command]
#[specta::specta]
pub async fn workspace_create_folder(
    state: State<'_, AppState>,
    name: String,
    parent_path: Option<String>,
) -> Result<WorkspaceNode> {
    run_blocking(state, move |w| {
        w.create_folder(&name, parent_path.as_deref().unwrap_or(""))
    })
    .await
}

/// Renames a workspace entry exclusively and dispatches `AppEvent::FileClosed` for its previous path.
#[tauri::command]
#[specta::specta]
pub async fn workspace_rename(
    state: State<'_, AppState>,
    relative_path: String,
    name: String,
) -> Result<WorkspaceNode> {
    let events = state.events.clone();
    run_blocking(state, move |w| {
        let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        let node = w.rename(&relative_path, &name)?;
        if let Some((root, path)) = old_target {
            events.dispatch(AppEvent::FileClosed { root, path });
        }
        Ok(node)
    })
    .await
}

/// Deletes a workspace entry and dispatches `AppEvent::FileClosed` for its path.
#[tauri::command]
#[specta::specta]
pub async fn workspace_delete(state: State<'_, AppState>, relative_path: String) -> Result<()> {
    let events = state.events.clone();
    run_blocking(state, move |w| {
        let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        w.delete(&relative_path)?;
        if let Some((root, path)) = old_target {
            events.dispatch(AppEvent::FileClosed { root, path });
        }
        Ok(())
    })
    .await
}

/// Marks the frontend quit lifecycle listener as ready.
#[tauri::command]
#[specta::specta]
pub fn workspace_frontend_ready(state: State<'_, AppState>) {
    state.frontend_ready.store(true, Ordering::Relaxed);
}

/// Shuts down all language servers and exits the Tauri application.
#[tauri::command]
pub fn workspace_finish_quit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) {
    state.lsp.shutdown_all();
    #[cfg(any(test, feature = "e2e-bridge"))]
    if std::env::var_os("LGTM_IPC_BRIDGE").is_some() {
        return;
    }
    app.exit(0);
}

/// Synchronizes in-memory editor changes (`textDocument/didChange`) with the language server.
#[tauri::command]
#[specta::specta]
pub async fn lsp_did_change(
    state: State<'_, AppState>,
    relative_path: String,
    content: String,
) -> Result<()> {
    let (root, file_path) = run_blocking(state.clone(), move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    run_lsp_blocking(state, move |s| s.did_change(&root, &file_path, &content)).await
}

/// Requests hover documentation (`textDocument/hover`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_hover(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Option<LspHoverResult>> {
    let (root, file_path) = run_blocking(state.clone(), move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    run_lsp_blocking(state, move |s| s.get_hover(&root, &file_path, offset)).await
}

/// Requests symbol definition locations (`textDocument/definition`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_goto_definition(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Vec<LspLocation>> {
    let (root, file_path) = run_blocking(state.clone(), move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    run_lsp_blocking(state, move |s| s.get_definition(&root, &file_path, offset)).await
}

/// Requests code completion candidates (`textDocument/completion`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_completion(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Vec<LspCompletionItem>> {
    let (root, file_path) = run_blocking(state.clone(), move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    run_lsp_blocking(state, move |s| s.get_completions(&root, &file_path, offset)).await
}

/// Requests callable signature help (`textDocument/signatureHelp`) at a UTF-16 editor offset.
#[tauri::command]
#[specta::specta]
pub async fn lsp_signature_help(
    state: State<'_, AppState>,
    relative_path: String,
    offset: u32,
) -> Result<Option<LspSignatureHelp>> {
    let (root, file_path) = run_blocking(state.clone(), move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    run_lsp_blocking(state, move |s| {
        s.get_signature_help(&root, &file_path, offset)
    })
    .await
}

/// Returns the current workspace LSP server statuses and diagnostics snapshot.
#[tauri::command]
#[specta::specta]
pub async fn lsp_get_state(state: State<'_, AppState>) -> Result<LspWorkspaceSnapshot> {
    let root = run_blocking(state.clone(), move |w| w.get_root()).await?;
    run_lsp_blocking(state, move |s| s.get_workspace_snapshot(&root)).await
}

/// Renders a read-only diagnostics buffer for the active workspace, optionally filtered by `language_id`.
#[tauri::command]
#[specta::specta]
pub async fn lsp_get_diagnostics_buffer(
    state: State<'_, AppState>,
    language_id: Option<String>,
) -> Result<LspDiagnosticsBuffer> {
    let root = run_blocking(state.clone(), move |w| w.get_root()).await?;
    run_lsp_blocking(state, move |s| {
        s.get_diagnostics_buffer(&root, language_id.as_deref())
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
    let root = run_blocking(state.clone(), move |w| w.get_root()).await?;
    run_lsp_blocking(state, move |s| {
        s.restart_language_server(&root, &language_id)
    })
    .await
}

/// Returns the resolved nested configuration snapshot across defaults, global settings, and project overrides.
#[tauri::command]
#[specta::specta]
pub async fn config_get(state: State<'_, AppState>) -> Result<ResolvedConfigSnapshot> {
    let config_service = state.config.clone();
    run_blocking(state, move |w| {
        let root = w.get_root().ok();
        config_service
            .get_config_snapshot(root.as_deref())
            .map_err(|err| error("io", err.to_string()))
    })
    .await
}

/// Loads the persisted session state (`~/.lgtm/sessions/<id>.json`).
#[tauri::command]
#[specta::specta]
pub async fn session_get(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Session> {
    let session_service = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(session_service.get_session(session_id.as_deref().unwrap_or("default"))?)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Saves the active sidebar widget, expanded explorer folders, and filtered tabs (`edited || last_opened`) for a workspace.
#[tauri::command]
#[specta::specta]
pub async fn session_save(
    state: State<'_, AppState>,
    patch: WorkspaceStatePatch,
) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = session_service.save_workspace_state("default", patch)?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Collapses or expands child worktrees for `project_id` in the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_set_collapsed(
    state: State<'_, AppState>,
    project_id: String,
    collapsed: bool,
) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let id = Uuid::parse_str(project_id.trim())
            .map_err(|err| error("invalidProject", err.to_string()))?;
        let session = session_service.set_project_collapsed("default", id, collapsed)?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Creates a `Managed` Git worktree under `~/.lgtm/worktrees/<project>/<branch-slug>` and attaches it to the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_create_worktree(
    state: State<'_, AppState>,
    project_id: String,
    branch: String,
    base_ref: Option<String>,
) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let id = Uuid::parse_str(project_id.trim())
            .map_err(|err| error("invalidProject", err.to_string()))?;
        let session =
            session_service.create_project_worktree("default", id, &branch, base_ref.as_deref())?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Removes a worktree from `project_id` (deleting disk storage when `Managed` and only detaching when `External`).
#[tauri::command]
#[specta::specta]
pub async fn project_remove_worktree(
    state: State<'_, AppState>,
    project_id: String,
    worktree_path: String,
    force: Option<bool>,
) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let id = Uuid::parse_str(project_id.trim())
            .map_err(|err| error("invalidProject", err.to_string()))?;
        let session = session_service.remove_project_worktree(
            "default",
            id,
            std::path::Path::new(worktree_path.trim()),
            force.unwrap_or(false),
        )?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

/// Removes `project_id` and its remembered workspace states from the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_remove(state: State<'_, AppState>, project_id: String) -> Result<Session> {
    let session_service = state.session.clone();
    let events = state.events.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let id = Uuid::parse_str(project_id.trim())
            .map_err(|err| error("invalidProject", err.to_string()))?;
        let session = session_service.remove_project("default", id)?;
        events.dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_specta_command_registry {
        use super::*;

        mod when_exporting_typescript_bindings {
            use super::*;

            #[test]
            fn should_generate_typescript_bindings_matching_nested_settings_and_commands() {
                // Arrange
                let target_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../lgtm-web/src/lib/gen/bindings.ts");

                // Act
                let generated = export_typescript_bindings_to(&target_path).unwrap();

                // Assert
                assert!(generated.contains("export type ThemeMode"));
                assert!(generated.contains("export type CommandStyle"));
                assert!(generated.contains("export type KeybindPreset"));
                assert!(generated.contains("export type KeybindingSettings"));
                assert!(generated.contains("export type KeybindingSettingsPatch"));
                assert!(generated.contains("export type AppearanceSettings"));
                assert!(generated.contains("export type WorkspaceSettings"));
                assert!(generated.contains("export type EditorSettings"));
                assert!(generated.contains("export type LspSettings"));
                assert!(generated.contains("export type LgtmConfig"));
                assert!(generated.contains("export type LgtmConfigPatch"));
                assert!(generated.contains("export type ResolvedConfigSnapshot"));
                assert!(generated.contains("export type FileOpenedEvent"));
                assert!(generated.contains("export type LspCompletionItem"));
                assert!(generated.contains("export type LspSignatureHelp"));
                assert!(generated.contains("lspCompletion"));
                assert!(generated.contains("lspSignatureHelp"));
                assert!(generated.contains("configGet"));
                assert!(generated.contains("configUpdate"));
            }
        }
    }
}
