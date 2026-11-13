//! Tauri application state, lifecycle, and Specta TypeScript binding generation.
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use lgtm_config::ConfigService;
use lgtm_lsp::LspEvent;
use lgtm_session::SessionService;
use lgtm_workspace::WorkspaceService;
use tauri::Manager;

use crate::{
    commands::*,
    events::{AppEvent, AppEvents, FileOpenedEvent},
    menus::{build_app_menu, handle_menu_event},
    services::LspService,
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

impl AppState {
    // Public ------------------------------------------------------------------

    /// Runs a blocking operation against cloned [`AppState`] handles on Tauri's blocking thread pool.
    pub async fn run_blocking<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Self) -> lgtm_workspace::Result<T> + Send + 'static,
    ) -> lgtm_workspace::Result<T> {
        let state = self.clone();
        tauri::async_runtime::spawn_blocking(move || operation(&state))
            .await
            .map_err(|err| lgtm_workspace::error("io", err.to_string()))?
    }

    /// Locks the shared [`WorkspaceService`] mutex, mapping poison errors to an IPC error.
    pub fn lock_workspace(
        &self,
    ) -> lgtm_workspace::Result<std::sync::MutexGuard<'_, WorkspaceService>> {
        self.workspace
            .lock()
            .map_err(|_| lgtm_workspace::error("io", "Workspace service is unavailable."))
    }

    /// Runs a mutating [`SessionService`] operation on the current thread and broadcasts [`AppEvent::SessionUpdated`].
    pub fn mutate_session(
        &self,
        operation: impl FnOnce(&SessionService) -> lgtm_workspace::Result<lgtm_session::Session>,
    ) -> lgtm_workspace::Result<lgtm_session::Session> {
        let session = operation(&self.session)?;
        self.events
            .dispatch(AppEvent::SessionUpdated(session.clone()));
        Ok(session)
    }
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
        config_update,
        session_get,
        session_save,
        project_set_collapsed,
        project_update_appearance,
        project_rename_worktree,
        project_create_worktree,
        project_remove_worktree,
        project_remove,
        project_checkout_branch,
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
            config_update,
            session_get,
            session_save,
            project_set_collapsed,
            project_update_appearance,
            project_rename_worktree,
            project_create_worktree,
            project_remove_worktree,
            project_remove,
            project_checkout_branch,
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
                assert!(generated.contains("sessionGet"));
                assert!(generated.contains("projectCreateWorktree"));
            }
        }
    }
}
