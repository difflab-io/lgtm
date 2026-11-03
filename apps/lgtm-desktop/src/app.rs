//! Tauri application state, lifecycle, and IPC command handlers.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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

/// Tracks whether the frontend has registered its quit lifecycle handlers.
#[derive(Default)]
pub struct FrontendReady(
    /// True after the frontend registers its quit handler.
    pub AtomicBool,
);

/// Construct the app; an explicit home is used by the isolated desktop smoke harness.
pub fn builder(workspace_home: Option<std::path::PathBuf>) -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .manage(FrontendReady::default())
        .setup(move |app| {
            let home = match workspace_home {
                Some(home) => home,
                None => app.path().home_dir()?,
            };
            app.manage(WorkspaceState(Arc::new(Mutex::new(
                WorkspaceService::from_home(home),
            ))));
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
    path: Option<String>,
) -> Result<WorkspaceListing> {
    run_blocking(state, move |w| w.open(path.as_deref())).await
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
    relative_path: String,
) -> Result<FileContent> {
    run_blocking(state, move |w| w.get_file_content(&relative_path)).await
}

#[tauri::command]
pub async fn workspace_write(
    state: State<'_, WorkspaceState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    run_blocking(state, move |w| w.write_file(&relative_path, &content)).await
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
    relative_path: String,
    name: String,
) -> Result<WorkspaceNode> {
    run_blocking(state, move |w| w.rename(&relative_path, &name)).await
}

#[tauri::command]
pub async fn workspace_delete(
    state: State<'_, WorkspaceState>,
    relative_path: String,
) -> Result<()> {
    run_blocking(state, move |w| w.delete(&relative_path)).await
}

#[tauri::command]
pub fn workspace_frontend_ready(state: State<'_, FrontendReady>) {
    state.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn workspace_finish_quit(app: tauri::AppHandle) {
    app.exit(0);
}
