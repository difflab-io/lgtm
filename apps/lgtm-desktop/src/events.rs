//! In-process backend event bus (`AppEvents` and `AppEvent`) wired in `app.rs` to coordinate backend services and webview notifications.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use lgtm_lsp::{LspFileDiagnosticsEvent, LspServerStatusEvent};
use lgtm_session::Session;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Emitter;

type WebviewSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
type EventHandler = Arc<dyn Fn(AppEvent) + Send + Sync>;

/// Payload emitted on `workspace-open-file` instructing the webview to open a file tab with an optional custom title and cursor offset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileOpenedEvent {
    /// Workspace-relative or config file path to open (`~/.lgtm/config.json`, `.lgtm/config.json`, or relative path).
    pub path: String,
    /// Optional custom tab title (such as `"Settings"` or `"Settings (project)"`).
    pub title: Option<String>,
    /// Optional UTF-16 character offset to position the cursor at after opening.
    pub offset: Option<u32>,
}

/// Strongly typed backend domain and lifecycle event dispatched through [`AppEvents`].
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// A workspace directory was opened, replacing `previous_root` with `next_root`.
    WorkspaceOpened {
        /// Previously active workspace root, if any.
        previous_root: Option<PathBuf>,
        /// Newly opened workspace root.
        next_root: PathBuf,
    },
    /// A file was requested to open in the webview editor with an optional custom tab title and offset.
    FileOpened(FileOpenedEvent),
    /// A file was written to disk by `WorkspaceService`.
    FileSaved {
        /// Active workspace root when the save occurred.
        root: PathBuf,
        /// Canonical path of the saved file.
        path: PathBuf,
        /// UTF-8 content written to disk.
        content: String,
        /// Whether `path` is `~/.lgtm/config.json` or `<workspace>/.lgtm/config.json`.
        is_config: bool,
    },
    /// A workspace entry was renamed or deleted, closing its previous document path.
    FileClosed {
        /// Active workspace root.
        root: PathBuf,
        /// Canonical path of the closed file or directory.
        path: PathBuf,
    },
    /// A language server transitioned status or updated its diagnostic counts.
    LspStatus(LspServerStatusEvent),
    /// A language server or config validator published diagnostics for a file.
    LspDiagnostics(LspFileDiagnosticsEvent),
    /// The persisted session or project tree changed.
    SessionUpdated(Session),
    /// The OS requested application exit (`RunEvent::ExitRequested`).
    QuitRequested,
}

/// In-process backend event bus that routes [`AppEvent`] variants to the central handler in `app.rs` and publishes notifications to webview sinks.
#[derive(Clone, Default)]
pub struct AppEvents {
    webview_sinks: Arc<RwLock<Vec<WebviewSink>>>,
    handler: Arc<RwLock<Option<EventHandler>>>,
}

impl AppEvents {
    /// Creates an `AppEvents` bus bound to `app_handle`.
    pub(crate) fn from_app_handle<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Self {
        let emit_handle = app_handle.clone();
        let events = Self::default();
        events.add_webview_sink(move |event, payload| {
            let _ = emit_handle.emit(event, payload);
        });
        events
    }

    // Public ------------------------------------------------------------------

    /// Registers the central backend event handler invoked on every [`AppEvents::dispatch`] call.
    pub fn set_handler<F>(&self, handler: F)
    where
        F: Fn(AppEvent) + Send + Sync + 'static,
    {
        if let Ok(mut slot) = self.handler.write() {
            *slot = Some(Arc::new(handler));
        }
    }

    /// Registers an outbound sink invoked whenever [`AppEvents::emit_to_webview`] publishes a notification.
    pub fn add_webview_sink<F>(&self, sink: F)
    where
        F: Fn(&str, serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut sinks) = self.webview_sinks.write() {
            sinks.push(Arc::new(sink));
        }
    }

    /// Dispatches a typed [`AppEvent`] to the registered backend handler in `app.rs`.
    pub fn dispatch(&self, event: AppEvent) {
        let handler = self.handler.read().ok().and_then(|slot| slot.clone());
        if let Some(handler) = handler {
            handler(event);
        }
    }

    /// Serializes `payload` and publishes a named notification to all registered webview sinks.
    pub fn emit_to_webview<T: serde::Serialize>(&self, event_name: &str, payload: &T) {
        if let Ok(value) = serde_json::to_value(payload)
            && let Ok(sinks) = self.webview_sinks.read()
        {
            for sink in sinks.iter() {
                sink(event_name, value.clone());
            }
        }
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // tests -------------------------------------------------------------------

    mod given_an_app_events_bus {
        use super::*;

        mod when_dispatching_events_and_emitting_to_webview {
            use super::*;

            #[test]
            fn should_invoke_registered_backend_handler_and_forward_webview_payloads() {
                // Arrange
                let webview_log: Arc<Mutex<Vec<(String, serde_json::Value)>>> =
                    Arc::new(Mutex::new(Vec::new()));
                let events = AppEvents::default();
                let sink_log = webview_log.clone();
                events.add_webview_sink(move |name, payload| {
                    sink_log.lock().unwrap().push((name.to_string(), payload));
                });
                let forward_events = events.clone();
                let closed_paths: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
                let recorded_closed = closed_paths.clone();
                events.set_handler(move |event| match event {
                    AppEvent::FileOpened(payload) => {
                        forward_events.emit_to_webview("workspace-open-file", &payload);
                    }
                    AppEvent::FileClosed { path, .. } => {
                        recorded_closed.lock().unwrap().push(path);
                    }
                    _ => {}
                });

                // Act
                events.dispatch(AppEvent::FileClosed {
                    root: PathBuf::from("/workspace"),
                    path: PathBuf::from("/workspace/README.md"),
                });
                events.dispatch(AppEvent::FileOpened(FileOpenedEvent {
                    path: "~/.lgtm/config.json".to_string(),
                    title: Some("Settings".to_string()),
                    offset: None,
                }));

                // Assert
                assert_eq!(
                    closed_paths.lock().unwrap().as_slice(),
                    &[PathBuf::from("/workspace/README.md")]
                );
                assert_eq!(webview_log.lock().unwrap().len(), 1);
                assert_eq!(webview_log.lock().unwrap()[0].0, "workspace-open-file");
            }
        }
    }
}
