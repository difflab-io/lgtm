//! Headless stdio IPC bridge service for Playwright E2E testing.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tauri::Manager;

use crate::app::{LspState, app_context, configure_builder, init_app_state};

/// Callback invoked whenever a backend Tauri event should be forwarded over the stdio IPC bridge.
pub type BridgeEventForwarder = Arc<dyn Fn(&str, Value) + Send + Sync>;

/// Headless stdio bridge service that routes newline-delimited JSON IPC invocations through Tauri's `MockRuntime`.
pub struct IpcBridgeService {
    home: PathBuf,
}

impl IpcBridgeService {
    /// Creates a new `IpcBridgeService` rooted at `home`.
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    // Public ------------------------------------------------------------------

    /// Starts the blocking stdin/stdout JSON-RPC loop against a `tauri::test::MockRuntime` application.
    pub fn run(&self) {
        let stdout_lock: Arc<Mutex<std::io::Stdout>> = Arc::new(Mutex::new(std::io::stdout()));
        let event_stdout = stdout_lock.clone();
        let forwarder: BridgeEventForwarder = Arc::new(move |event_name, payload| {
            let line = json!({
                "event": event_name,
                "payload": payload,
            });
            if let Ok(mut out) = event_stdout.lock() {
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        });

        let app = configure_builder(tauri::test::mock_builder(), Some(self.home.clone()))
            .build(app_context())
            .expect("failed to build mock Tauri app for IPC bridge");
        init_app_state(app.handle(), Some(self.home.clone()), Some(forwarder))
            .expect("failed to initialize IPC bridge state");
        let webview = app.get_webview_window("main").unwrap_or_else(|| {
            tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                .build()
                .expect("failed to build mock webview window for IPC bridge")
        });
        let ipc_url: tauri::Url = if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("valid IPC origin URL");

        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(raw) = line else {
                break;
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            let id = msg.get("id").and_then(Value::as_u64).unwrap_or(0);
            let cmd = msg
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let args = msg
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()));

            let response = tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd,
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: ipc_url.clone(),
                    body: tauri::ipc::InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            );

            let reply = match response {
                Ok(body) => {
                    let value = body.deserialize::<Value>().unwrap_or(Value::Null);
                    json!({ "id": id, "ok": value })
                }
                Err(err_val) => {
                    json!({ "id": id, "error": err_val })
                }
            };

            if let Ok(mut out) = stdout_lock.lock() {
                let _ = writeln!(out, "{reply}");
                let _ = out.flush();
            }
        }

        app.state::<LspState>().0.shutdown_all();
    }
}
