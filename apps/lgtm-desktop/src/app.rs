//! Tauri application state, lifecycle, and IPC command handlers.
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use lgtm_config::ConfigService;
use lgtm_lsp::{
    LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation, LspService, LspWorkspaceSnapshot,
};
use tauri::{Emitter, Manager, State};

use crate::{
    error::{Result, error},
    models::{FileContent, WorkspaceListing, WorkspaceNode},
    services::WorkspaceService,
};

/// Shared workspace access for Tauri command handlers.
pub struct WorkspaceState(
    /// The shared workspace service guarded across all Tauri file commands.
    pub Arc<Mutex<WorkspaceService>>,
);

/// Shared LSP service access for Tauri command handlers.
pub struct LspState(
    /// The shared LSP orchestration service across all Tauri workspace and editor commands.
    pub Arc<LspService>,
);

/// Tracks whether the frontend has registered its quit lifecycle handlers.
#[derive(Default)]
pub struct FrontendReady(
    /// True after the frontend registers its quit handler.
    pub AtomicBool,
);

/// Construct the app; an explicit home is used by the isolated desktop smoke harness.
pub fn builder(workspace_home: Option<PathBuf>) -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .manage(FrontendReady::default())
        .setup(move |app| {
            let home = match workspace_home {
                Some(home) => home,
                None => app.path().home_dir()?,
            };
            let global_config_path = home.join(".lgtm").join("config.toml");
            let config_service = ConfigService::with_global_config_path(global_config_path);
            let app_handle = app.handle().clone();
            let lsp_service = LspService::new(config_service, move |event| match event {
                LspEvent::Status(payload) => {
                    let _ = app_handle.emit("lsp-status", payload);
                }
                LspEvent::Diagnostics(payload) => {
                    let _ = app_handle.emit("lsp-diagnostics", payload);
                }
            });
            app.manage(WorkspaceState(Arc::new(Mutex::new(
                WorkspaceService::from_home(home),
            ))));
            app.manage(LspState(Arc::new(lsp_service)));
            Ok(())
        })
        .invoke_handler(commands)
}

/// Shared command dispatch for the app and the native integration harness.
pub fn commands(invoke: tauri::ipc::Invoke) -> bool {
    let handler: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
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
        lsp_get_state,
        lsp_get_diagnostics_buffer,
    ];
    handler(invoke)
}

/// User-requested quit coordinates with the frontend before exiting.
pub fn handle_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested {
        code: None, api, ..
    } = event
        && app.state::<FrontendReady>().0.load(Ordering::Relaxed)
        && !app.webview_windows().is_empty()
    {
        api.prevent_exit();
        let _ = app.emit("workspace-quit-requested", ());
    }
}

/// Builds and runs the Tauri desktop application.
pub fn run() {
    builder(None)
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(handle_event);
}

async fn run_blocking<T: Send + 'static>(
    state: State<'_, WorkspaceState>,
    operation: impl FnOnce(&mut WorkspaceService) -> Result<T> + Send + 'static,
) -> Result<T> {
    let workspace = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut locked = workspace
            .lock()
            .map_err(|_| error("io", "Workspace service is unavailable."))?;
        operation(&mut locked)
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn workspace_open(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    path: Option<String>,
) -> Result<WorkspaceListing> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let listing = w.open(path.as_deref())?;
        if let Ok(root) = w.get_root() {
            let _ = lsp_service.prepare_workspace(&root);
        }
        Ok(listing)
    })
    .await
}

#[tauri::command]
pub async fn workspace_pick_folder() -> Result<Option<String>> {
    Ok(rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|folder| folder.path().to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn workspace_list(
    state: State<'_, WorkspaceState>,
    relative_path: String,
) -> Result<Vec<WorkspaceNode>> {
    run_blocking(state, move |w| w.get_entries(&relative_path)).await
}

#[tauri::command]
pub async fn workspace_read(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
) -> Result<FileContent> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let content = w.get_file_content(&relative_path)?;
        let root = w.get_root()?;
        let file_path = w.get_entry_path(&relative_path)?;
        let (language_id, lsp_status) =
            lsp_service.open_or_sync_file(&root, &file_path, &content.content);
        Ok(content.with_lsp(language_id, lsp_status))
    })
    .await
}

#[tauri::command]
pub async fn workspace_write(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let written = w.write_file(&relative_path, &content)?;
        if let (Ok(root), Ok(file_path)) = (w.get_root(), w.get_entry_path(&relative_path)) {
            let _ = lsp_service.did_save(&root, &file_path, &content);
        }
        Ok(written)
    })
    .await
}

#[tauri::command]
pub async fn workspace_create_file(
    state: State<'_, WorkspaceState>,
    name: String,
    parent_path: Option<String>,
) -> Result<WorkspaceNode> {
    run_blocking(state, move |w| {
        w.create_file(&name, parent_path.as_deref().unwrap_or(""))
    })
    .await
}

#[tauri::command]
pub async fn workspace_create_folder(
    state: State<'_, WorkspaceState>,
    name: String,
    parent_path: Option<String>,
) -> Result<WorkspaceNode> {
    run_blocking(state, move |w| {
        w.create_folder(&name, parent_path.as_deref().unwrap_or(""))
    })
    .await
}

#[tauri::command]
pub async fn workspace_rename(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    name: String,
) -> Result<WorkspaceNode> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let old_coords = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        let renamed = w.rename(&relative_path, &name)?;
        if let Some((root, old_path)) = old_coords {
            let _ = lsp_service.did_close(&root, &old_path);
        }
        Ok(renamed)
    })
    .await
}

#[tauri::command]
pub async fn workspace_delete(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
) -> Result<()> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let old_coords = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        w.delete(&relative_path)?;
        if let Some((root, old_path)) = old_coords {
            let _ = lsp_service.did_close(&root, &old_path);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn workspace_frontend_ready(state: State<'_, FrontendReady>) {
    state.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn workspace_finish_quit(app: tauri::AppHandle, lsp: State<'_, LspState>) {
    lsp.0.shutdown_all();
    app.exit(0);
}

#[tauri::command]
pub async fn lsp_did_change(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    content: String,
) -> Result<()> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let root = w.get_root()?;
        let file_path = w.get_entry_path(&relative_path)?;
        lsp_service
            .did_change(&root, &file_path, &content)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn lsp_hover(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    offset: u32,
) -> Result<Option<LspHoverResult>> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let root = w.get_root()?;
        let file_path = w.get_entry_path(&relative_path)?;
        lsp_service
            .get_hover(&root, &file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn lsp_goto_definition(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    offset: u32,
) -> Result<Vec<LspLocation>> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let root = w.get_root()?;
        let file_path = w.get_entry_path(&relative_path)?;
        lsp_service
            .get_definition(&root, &file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn lsp_get_state(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
) -> Result<LspWorkspaceSnapshot> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let root = w.get_root()?;
        lsp_service
            .get_workspace_snapshot(&root)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn lsp_get_diagnostics_buffer(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    language_id: Option<String>,
) -> Result<LspDiagnosticsBuffer> {
    let lsp_service = lsp.0.clone();
    run_blocking(state, move |w| {
        let root = w.get_root()?;
        lsp_service
            .get_diagnostics_buffer(&root, language_id.as_deref())
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
}
