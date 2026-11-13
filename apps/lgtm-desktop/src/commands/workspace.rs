//! Workspace filesystem and application quit lifecycle IPC command handlers.

use std::sync::atomic::Ordering;

use lgtm_workspace::{FileContent, Result, WorkspaceListing, WorkspaceNode};
use tauri::State;

use crate::{app::AppState, events::AppEvent};

/// Opens the active or specified workspace directory, updates the `default` session, and prepares workspace language servers.
#[tauri::command]
#[specta::specta]
pub async fn workspace_open(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<WorkspaceListing> {
    state
        .run_blocking(move |app| {
            let mut w = app.lock_workspace()?;
            let previous_root = w.get_root().ok();
            let effective_path = match path {
                Some(explicit) => Some(explicit),
                None if !w.is_custom_root() => app
                    .session
                    .get_session("default")
                    .ok()
                    .map(|s| s.active_workspace_root)
                    .filter(|active| {
                        !active.trim().is_empty() && std::path::Path::new(active).is_dir()
                    })
                    .or_else(|| {
                        app.config
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
                if let Ok(session) = app.session.open_workspace("default", &next_root) {
                    app.events.dispatch(AppEvent::SessionUpdated(session));
                }
                app.events.dispatch(AppEvent::WorkspaceOpened {
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
    state
        .run_blocking(move |app| app.lock_workspace()?.get_entries(&relative_path))
        .await
}

/// Reads UTF-8 file contents and synchronizes the document with its language server.
#[tauri::command]
#[specta::specta]
pub async fn workspace_read(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<FileContent> {
    state
        .run_blocking(move |app| {
            let (mut file, root_and_path) = {
                let w = app.lock_workspace()?;
                let file = w.get_file_content(&relative_path)?;
                let root_and_path = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
                (file, root_and_path)
            };
            if let Some((root, path)) = root_and_path {
                let (language_id, lsp_status) =
                    app.lsp.open_or_sync_file(&root, &path, &file.content);
                file.language_id = language_id;
                file.lsp_status = lsp_status;
            }
            Ok(file)
        })
        .await
}

/// Writes UTF-8 file contents to disk and dispatches `AppEvent::FileSaved`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_write(
    state: State<'_, AppState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    state
        .run_blocking(move |app| {
            let (saved, save_context) = {
                let w = app.lock_workspace()?;
                let saved = w.write_file(&relative_path, &content)?;
                let context = w
                    .get_root()
                    .ok()
                    .zip(w.get_entry_path(&relative_path).ok())
                    .map(|(root, path)| {
                        let is_config = w.is_config_path(&path);
                        (root, path, is_config)
                    });
                (saved, context)
            };
            if let Some((root, path, is_config)) = save_context {
                app.events.dispatch(AppEvent::FileSaved {
                    root,
                    path,
                    content,
                    is_config,
                });
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
    state
        .run_blocking(move |app| {
            app.lock_workspace()?
                .create_file(&name, parent_path.as_deref().unwrap_or(""))
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
    state
        .run_blocking(move |app| {
            app.lock_workspace()?
                .create_folder(&name, parent_path.as_deref().unwrap_or(""))
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
    state
        .run_blocking(move |app| {
            let w = app.lock_workspace()?;
            let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
            let node = w.rename(&relative_path, &name)?;
            if let Some((root, path)) = old_target {
                app.events.dispatch(AppEvent::FileClosed { root, path });
            }
            Ok(node)
        })
        .await
}

/// Deletes a workspace entry and dispatches `AppEvent::FileClosed` for its path.
#[tauri::command]
#[specta::specta]
pub async fn workspace_delete(state: State<'_, AppState>, relative_path: String) -> Result<()> {
    state
        .run_blocking(move |app| {
            let w = app.lock_workspace()?;
            let old_target = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
            w.delete(&relative_path)?;
            if let Some((root, path)) = old_target {
                app.events.dispatch(AppEvent::FileClosed { root, path });
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
