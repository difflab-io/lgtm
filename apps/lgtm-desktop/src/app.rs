//! Tauri application state, lifecycle, Specta TypeScript binding generation, and IPC command handlers.
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use lgtm_config::{ConfigService, ResolvedConfigSnapshot};
use lgtm_lsp::{
    LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation, LspServerStatusEvent,
    LspWorkspaceSnapshot,
};
use tauri::{Emitter, Manager, State};

#[cfg(any(test, feature = "e2e-bridge"))]
use crate::services::BridgeEventForwarder;
use crate::{
    error::{Result, error},
    models::{FileContent, WorkspaceListing, WorkspaceNode},
    services::{LspService, WorkspaceService},
};

type EventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Application-wide event emitter wired in `app.rs` to publish backend events to the webview and test bridge.
#[derive(Clone)]
pub struct AppEvents(EventSink);

impl AppEvents {
    #[cfg(any(test, feature = "e2e-bridge"))]
    pub(crate) fn from_app_handle<R: tauri::Runtime>(
        app_handle: &tauri::AppHandle<R>,
        event_forwarder: Option<BridgeEventForwarder>,
    ) -> Self {
        let emit_handle = app_handle.clone();
        Self(Arc::new(move |event, payload| {
            if let Some(cb) = &event_forwarder {
                cb(event, payload.clone());
            }
            let _ = emit_handle.emit(event, payload);
        }))
    }

    #[cfg(not(any(test, feature = "e2e-bridge")))]
    pub(crate) fn from_app_handle<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Self {
        let emit_handle = app_handle.clone();
        Self(Arc::new(move |event, payload| {
            let _ = emit_handle.emit(event, payload);
        }))
    }

    // Public ------------------------------------------------------------------

    /// Emits a named desktop event to the frontend webview and any registered test bridge forwarder.
    pub fn emit<T: serde::Serialize>(&self, event: &str, payload: &T) {
        if let Ok(value) = serde_json::to_value(payload) {
            (self.0)(event, value);
        }
    }
}

/// Combined desktop host state managed by Tauri across all IPC command handlers.
#[derive(Clone)]
pub struct AppState {
    /// Shared workspace filesystem service guarded across file and config operations.
    pub workspace: Arc<Mutex<WorkspaceService>>,
    /// Shared configuration service resolving global and workspace settings.
    pub config: ConfigService,
    /// Shared desktop LSP orchestration service.
    pub lsp: Arc<LspService>,
    /// Application event sink publishing events to the frontend webview.
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
                    match event.id().as_ref() {
                        "open-global-config" => {
                            state.events.emit("workspace-open-global-config", &());
                        }
                        "open-local-config" => {
                            state.events.emit("workspace-open-local-config", &());
                        }
                        _ => {}
                    }
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
            #[cfg(any(test, feature = "e2e-bridge"))]
            init_app_state(app.handle(), workspace_home, None)?;
            #[cfg(not(any(test, feature = "e2e-bridge")))]
            init_app_state(app.handle(), workspace_home)?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(commands)
}

fn build_app_menu<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let pkg_info = app_handle.package_info();
    let config = app_handle.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let settings_item = MenuItem::with_id(
        app_handle,
        "open-global-config",
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let project_settings_item = MenuItem::with_id(
        app_handle,
        "open-local-config",
        "Project Settings…",
        true,
        Some("CmdOrCtrl+Shift+,"),
    )?;

    let window_menu = Submenu::with_id_and_items(
        app_handle,
        tauri::menu::WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app_handle, None)?,
            &PredefinedMenuItem::maximize(app_handle, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::close_window(app_handle, None)?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        app_handle,
        tauri::menu::HELP_SUBMENU_ID,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app_handle, None, Some(about_metadata.clone()))?,
        ],
    )?;

    Menu::with_items(
        app_handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app_handle, None, Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &settings_item,
                    &project_settings_item,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::services(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::hide(app_handle, None)?,
                    &PredefinedMenuItem::hide_others(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::quit(app_handle, None)?,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                app_handle,
                "File",
                true,
                &[
                    #[cfg(not(target_os = "macos"))]
                    &settings_item,
                    #[cfg(not(target_os = "macos"))]
                    &project_settings_item,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::close_window(app_handle, None)?,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::quit(app_handle, None)?,
                ],
            )?,
            &Submenu::with_items(
                app_handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app_handle, None)?,
                    &PredefinedMenuItem::redo(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::cut(app_handle, None)?,
                    &PredefinedMenuItem::copy(app_handle, None)?,
                    &PredefinedMenuItem::paste(app_handle, None)?,
                    &PredefinedMenuItem::select_all(app_handle, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app_handle, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )
}

pub(crate) fn init_app_state<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    workspace_home: Option<PathBuf>,
    #[cfg(any(test, feature = "e2e-bridge"))] event_forwarder: Option<BridgeEventForwarder>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if app_handle.try_state::<AppState>().is_some() {
        return Ok(());
    }
    let home = match workspace_home {
        Some(home) => home,
        None => app_handle.path().home_dir()?,
    };
    let config = ConfigService::from_home(home.clone());
    #[cfg(any(test, feature = "e2e-bridge"))]
    let events = AppEvents::from_app_handle(app_handle, event_forwarder);
    #[cfg(not(any(test, feature = "e2e-bridge")))]
    #[cfg(not(any(test, feature = "e2e-bridge")))]
    let events = AppEvents::from_app_handle(app_handle);
    let lsp_events = events.clone();
    let lsp = Arc::new(LspService::new(config.clone(), move |event| match event {
        LspEvent::Status(payload) => lsp_events.emit("lsp-status", &payload),
        LspEvent::Diagnostics(payload) => lsp_events.emit("lsp-diagnostics", &payload),
    }));
    app_handle.manage(AppState {
        workspace: Arc::new(Mutex::new(WorkspaceService::from_home(home))),
        config,
        lsp,
        events,
        frontend_ready: Arc::new(AtomicBool::new(false)),
    });
    Ok(())
}

/// Shared command dispatch for the app and the native integration harness.
pub fn commands<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        workspace_open,
        workspace_pick_folder,
        workspace_list,
        workspace_read,
        workspace_open_global_config,
        workspace_open_local_config,
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
        lsp_get_state,
        lsp_get_diagnostics_buffer,
        lsp_restart,
        config_get,
    ];
    handler(invoke)
}

/// Builds the `tauri-specta` command and type registry for exporting TypeScript bindings.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .disable_serde_phases()
        .dangerously_cast_bigints_to_number()
        .commands(tauri_specta::collect_commands![
            workspace_open,
            workspace_pick_folder,
            workspace_list,
            workspace_read,
            workspace_open_global_config,
            workspace_open_local_config,
            workspace_write,
            workspace_create_file,
            workspace_create_folder,
            workspace_rename,
            workspace_delete,
            workspace_frontend_ready,
            lsp_did_change,
            lsp_hover,
            lsp_goto_definition,
            lsp_get_state,
            lsp_get_diagnostics_buffer,
            lsp_restart,
            config_get,
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
        state.events.emit("workspace-quit-requested", &());
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

/// Opens the active or specified workspace directory and prepares workspace language servers.
#[tauri::command]
#[specta::specta]
pub async fn workspace_open(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<WorkspaceListing> {
    let config_service = state.config.clone();
    let lsp_service = state.lsp.clone();
    run_blocking(state, move |w| {
        let prev_root = w.get_root().ok();
        let effective_path = match path {
            Some(explicit) => Some(explicit),
            None if !w.is_custom_root() => config_service
                .get_settings_snapshot(None)
                .ok()
                .and_then(|snap| snap.effective.workspace.active_root)
                .filter(|active| {
                    !active.trim().is_empty() && std::path::Path::new(active).is_dir()
                }),
            None => None,
        };
        let listing = w.open(effective_path.as_deref())?;
        if let Ok(new_root) = w.get_root() {
            lsp_service.switch_workspace(prev_root.as_deref(), &new_root);
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
    run_blocking(state, move |w| {
        let mut file = w.get_file_content(&relative_path)?;
        if let (Ok(root), Ok(path)) = (w.get_root(), w.get_entry_path(&relative_path)) {
            let (language_id, lsp_status) =
                lsp_service.open_or_sync_file(&root, &path, &file.content);
            file.language_id = language_id;
            file.lsp_status = lsp_status;
        }
        Ok(file)
    })
    .await
}

/// Ensures the global `~/.lgtm/config.json` file exists, synchronizes LSP, and returns its contents.
#[tauri::command]
#[specta::specta]
pub async fn workspace_open_global_config(state: State<'_, AppState>) -> Result<FileContent> {
    let lsp_service = state.lsp.clone();
    run_blocking(state, move |w| {
        let mut file = w.open_global_config()?;
        if let (Ok(root), Ok(path)) = (w.get_root(), w.get_entry_path("~/.lgtm/config.json")) {
            let (language_id, lsp_status) =
                lsp_service.open_or_sync_file(&root, &path, &file.content);
            file.language_id = language_id;
            file.lsp_status = lsp_status;
        }
        Ok(file)
    })
    .await
}

/// Ensures the workspace `.lgtm/config.json` file exists, synchronizes LSP, and returns its contents.
#[tauri::command]
#[specta::specta]
pub async fn workspace_open_local_config(state: State<'_, AppState>) -> Result<FileContent> {
    let lsp_service = state.lsp.clone();
    run_blocking(state, move |w| {
        let mut file = w.open_local_config()?;
        if let (Ok(root), Ok(path)) = (w.get_root(), w.get_entry_path(".lgtm/config.json")) {
            let (language_id, lsp_status) =
                lsp_service.open_or_sync_file(&root, &path, &file.content);
            file.language_id = language_id;
            file.lsp_status = lsp_status;
        }
        Ok(file)
    })
    .await
}

/// Writes UTF-8 file contents to disk, notifies LSP `didSave`, and emits `config-updated` when a config file is saved.
#[tauri::command]
#[specta::specta]
pub async fn workspace_write(
    state: State<'_, AppState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    let lsp_service = state.lsp.clone();
    let config_service = state.config.clone();
    let events = state.events.clone();
    run_blocking(state, move |w| {
        let saved = w.write_file(&relative_path, &content)?;
        if let (Ok(root), Ok(path)) = (w.get_root(), w.get_entry_path(&relative_path)) {
            let _ = lsp_service.did_save(&root, &path, &content);
            if w.is_config_path(&path)
                && let Ok(snapshot) = config_service.get_settings_snapshot(Some(&root))
            {
                events.emit("config-updated", &snapshot);
            }
        }
        Ok(saved)
    })
    .await
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

/// Renames a workspace entry exclusively and closes its previous LSP document URI.
#[tauri::command]
#[specta::specta]
pub async fn workspace_rename(
    state: State<'_, AppState>,
    relative_path: String,
    name: String,
) -> Result<WorkspaceNode> {
    let lsp_service = state.lsp.clone();
    run_blocking(state, move |w| {
        let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        let node = w.rename(&relative_path, &name)?;
        if let Some((root, old_path)) = old_target {
            let _ = lsp_service.did_close(&root, &old_path);
        }
        Ok(node)
    })
    .await
}

/// Deletes a workspace entry and closes its LSP document URI.
#[tauri::command]
#[specta::specta]
pub async fn workspace_delete(state: State<'_, AppState>, relative_path: String) -> Result<()> {
    let lsp_service = state.lsp.clone();
    run_blocking(state, move |w| {
        let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        w.delete(&relative_path)?;
        if let Some((root, old_path)) = old_target {
            let _ = lsp_service.did_close(&root, &old_path);
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
            .get_settings_snapshot(root.as_deref())
            .map_err(|err| error("io", err.to_string()))
    })
    .await
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lgtm_config::{LineWrap, ThemeMode};
    use tempfile::TempDir;

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
                assert!(generated.contains("export type AppearanceSettings"));
                assert!(generated.contains("export type WorkspaceSettings"));
                assert!(generated.contains("export type EditorSettings"));
                assert!(generated.contains("export type UserSettings"));
                assert!(generated.contains("export type UserSettingsPatch"));
                assert!(generated.contains("export type ResolvedConfigSnapshot"));
                assert!(generated.contains("workspaceOpenGlobalConfig"));
                assert!(generated.contains("workspaceOpenLocalConfig"));
                assert!(generated.contains("configGet"));
            }
        }
    }

    mod given_workspace_and_config_services {
        use super::*;

        mod when_opening_and_editing_global_and_local_config_files {
            use super::*;

            #[test]
            fn should_initialize_config_files_on_workspace_service_and_resolve_merged_snapshot() {
                // Arrange
                let temp = TempDir::new().unwrap();
                let mut workspace = WorkspaceService::from_home(temp.path().to_path_buf());
                workspace.open(None).unwrap();
                let root = workspace.get_root().unwrap();
                let config_service = ConfigService::from_home(temp.path().to_path_buf());

                // Act
                let initial = config_service.get_settings_snapshot(Some(&root)).unwrap();
                let global_doc = workspace.open_global_config().unwrap();
                let local_doc = workspace.open_local_config().unwrap();
                let written_global = workspace
                    .write_file(
                        "~/.lgtm/config.json",
                        "{\n  \"appearance\": {\n    \"theme\": \"dark\"\n  },\n  \"editor\": {\n    \"lineWrapping\": \"off\"\n  }\n}\n",
                    )
                    .unwrap();
                let written_local = workspace
                    .write_file(
                        ".lgtm/config.json",
                        "{\n  \"appearance\": {\n    \"theme\": \"light\"\n  }\n}\n",
                    )
                    .unwrap();
                let resolved = config_service.get_settings_snapshot(Some(&root)).unwrap();

                // Assert
                assert_eq!(initial.effective.appearance.theme, ThemeMode::System);
                assert_eq!(initial.effective.editor.line_wrapping, LineWrap::Auto);
                assert!(global_doc.content.contains("\"theme\": \"system\""));
                assert!(local_doc.content.contains("\"theme\": \"system\""));
                assert_ne!(global_doc.revision, written_global.revision);
                assert_ne!(local_doc.revision, written_local.revision);
                assert!(
                    workspace
                        .is_config_path(&workspace.get_entry_path("~/.lgtm/config.json").unwrap())
                );
                assert!(
                    workspace
                        .is_config_path(&workspace.get_entry_path(".lgtm/config.json").unwrap())
                );
                assert_eq!(resolved.effective.appearance.theme, ThemeMode::Light);
                assert_eq!(resolved.effective.editor.line_wrapping, LineWrap::Off);
            }
        }
    }
}
