//! Tauri application state, lifecycle, and IPC command handlers.
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use lgtm_config::ConfigService;
use lgtm_lsp::{
    LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation, LspService, LspWorkspaceSnapshot,
};
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};

use crate::{
    error::{Result, error},
    models::{FileContent, WorkspaceListing, WorkspaceNode},
    services::WorkspaceService,
};

type BridgeEventForwarder = Arc<dyn Fn(&str, Value) + Send + Sync>;

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
    configure_builder(tauri::Builder::default(), workspace_home, None)
}

/// Configure any Tauri runtime builder with real workspace and LSP services and IPC handlers.
pub fn configure_builder<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    workspace_home: Option<PathBuf>,
    event_forwarder: Option<BridgeEventForwarder>,
) -> tauri::Builder<R> {
    builder
        .manage(FrontendReady::default())
        .setup(move |app| {
            init_app_state(app.handle(), workspace_home, event_forwarder)?;
            Ok(())
        })
        .invoke_handler(commands)
}

fn init_app_state<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    workspace_home: Option<PathBuf>,
    event_forwarder: Option<BridgeEventForwarder>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if app_handle.try_state::<WorkspaceState>().is_some() {
        return Ok(());
    }
    let home = match workspace_home {
        Some(home) => home,
        None => app_handle.path().home_dir()?,
    };
    let global_config_path = home.join(".lgtm").join("config.toml");
    let config_service = ConfigService::with_global_config_path(global_config_path);
    let emit_handle = app_handle.clone();
    let lsp_service = LspService::new(config_service, move |event| match event {
        LspEvent::Status(payload) => {
            if let Some(cb) = &event_forwarder
                && let Ok(val) = serde_json::to_value(&payload)
            {
                cb("lsp-status", val);
            }
            let _ = emit_handle.emit("lsp-status", payload);
        }
        LspEvent::Diagnostics(payload) => {
            if let Some(cb) = &event_forwarder
                && let Ok(val) = serde_json::to_value(&payload)
            {
                cb("lsp-diagnostics", val);
            }
            let _ = emit_handle.emit("lsp-diagnostics", payload);
        }
    });
    app_handle.manage(WorkspaceState(Arc::new(Mutex::new(
        WorkspaceService::from_home(home),
    ))));
    app_handle.manage(LspState(Arc::new(lsp_service)));
    Ok(())
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
        lsp_get_state,
        lsp_get_diagnostics_buffer,
    ];
    handler(invoke)
}

fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// Runs a headless stdio IPC bridge backed by the real Tauri command router and services.
pub fn run_ipc_bridge(home: PathBuf) {
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

    let app = configure_builder(
        tauri::test::mock_builder(),
        Some(home.clone()),
        Some(forwarder.clone()),
    )
    .build(app_context())
    .expect("failed to build mock Tauri app for IPC bridge");
    init_app_state(app.handle(), Some(home), Some(forwarder))
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
        .build(app_context())
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
    let (listing, root) = run_blocking(state, move |w| {
        let listing = w.open(path.as_deref())?;
        let root = w.get_root().ok();
        Ok((listing, root))
    })
    .await?;
    if let Some(root) = root {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = lsp_service.prepare_workspace(&root);
        })
        .await
        .map_err(|e| error("io", e.to_string()))?;
    }
    Ok(listing)
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
    let (content, root, file_path) = run_blocking(state, move |w| {
        let content = w.get_file_content(&relative_path)?;
        let root = w.get_root()?;
        let file_path = w.get_entry_path(&relative_path)?;
        Ok((content, root, file_path))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let (language_id, lsp_status) =
            lsp_service.open_or_sync_file(&root, &file_path, &content.content);
        Ok(content.with_lsp(language_id, lsp_status))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn workspace_write(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    content: String,
) -> Result<FileContent> {
    let lsp_service = lsp.0.clone();
    let content_for_lsp = content.clone();
    let (written, coords) = run_blocking(state, move |w| {
        let written = w.write_file(&relative_path, &content)?;
        let coords = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        Ok((written, coords))
    })
    .await?;
    if let Some((root, file_path)) = coords {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = lsp_service.did_save(&root, &file_path, &content_for_lsp);
        })
        .await
        .map_err(|e| error("io", e.to_string()))?;
    }
    Ok(written)
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
    let (renamed, old_coords) = run_blocking(state, move |w| {
        let old_coords = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        let renamed = w.rename(&relative_path, &name)?;
        Ok((renamed, old_coords))
    })
    .await?;
    if let Some((root, old_path)) = old_coords {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = lsp_service.did_close(&root, &old_path);
        })
        .await
        .map_err(|e| error("io", e.to_string()))?;
    }
    Ok(renamed)
}

#[tauri::command]
pub async fn workspace_delete(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
) -> Result<()> {
    let lsp_service = lsp.0.clone();
    let old_coords = run_blocking(state, move |w| {
        let old_coords = w.get_root().ok().zip(w.get_entry_path(&relative_path).ok());
        w.delete(&relative_path)?;
        Ok(old_coords)
    })
    .await?;
    if let Some((root, old_path)) = old_coords {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = lsp_service.did_close(&root, &old_path);
        })
        .await
        .map_err(|e| error("io", e.to_string()))?;
    }
    Ok(())
}

#[tauri::command]
pub fn workspace_frontend_ready(state: State<'_, FrontendReady>) {
    state.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn workspace_finish_quit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lsp: State<'_, LspState>,
) {
    lsp.0.shutdown_all();
    if std::env::var_os("LGTM_IPC_BRIDGE").is_none() {
        app.exit(0);
    }
}

#[tauri::command]
pub async fn lsp_did_change(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    content: String,
) -> Result<()> {
    let lsp_service = lsp.0.clone();
    let (root, file_path) = run_blocking(state, move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        lsp_service
            .did_change(&root, &file_path, &content)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn lsp_hover(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    offset: u32,
) -> Result<Option<LspHoverResult>> {
    let lsp_service = lsp.0.clone();
    let (root, file_path) = run_blocking(state, move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        lsp_service
            .get_hover(&root, &file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn lsp_goto_definition(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    relative_path: String,
    offset: u32,
) -> Result<Vec<LspLocation>> {
    let lsp_service = lsp.0.clone();
    let (root, file_path) = run_blocking(state, move |w| {
        Ok((w.get_root()?, w.get_entry_path(&relative_path)?))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        lsp_service
            .get_definition(&root, &file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn lsp_get_state(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
) -> Result<LspWorkspaceSnapshot> {
    let lsp_service = lsp.0.clone();
    let root = run_blocking(state, move |w| w.get_root()).await?;
    tauri::async_runtime::spawn_blocking(move || {
        lsp_service
            .get_workspace_snapshot(&root)
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

#[tauri::command]
pub async fn lsp_get_diagnostics_buffer(
    state: State<'_, WorkspaceState>,
    lsp: State<'_, LspState>,
    language_id: Option<String>,
) -> Result<LspDiagnosticsBuffer> {
    let lsp_service = lsp.0.clone();
    let root = run_blocking(state, move |w| w.get_root()).await?;
    tauri::async_runtime::spawn_blocking(move || {
        lsp_service
            .get_diagnostics_buffer(&root, language_id.as_deref())
            .map_err(|e| error("lsp", e.to_string()))
    })
    .await
    .map_err(|e| error("io", e.to_string()))?
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;
    use tempfile::TempDir;

    // fixtures ----------------------------------------------------------------

    fn write_mock_lsp_server(dir: &Path) -> PathBuf {
        let script_path = dir.join("mock-lsp.mjs");
        let script = r#"import { Buffer } from 'node:buffer';

let buffer = Buffer.alloc(0);

function send(msg) {
  const body = Buffer.from(JSON.stringify(msg), 'utf8');
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function handleMessage(msg) {
  if (msg.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        capabilities: {
          textDocumentSync: 1,
          hoverProvider: true,
          definitionProvider: true,
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/didOpen' || msg.method === 'textDocument/didChange') {
    const uri = msg.params.textDocument.uri;
    const text = msg.method === 'textDocument/didOpen'
      ? msg.params.textDocument.text
      : msg.params.contentChanges[0].text;
    const diagnostics = [];
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      const idx = lines[i].indexOf('BROKEN');
      if (idx !== -1) {
        diagnostics.push({
          range: {
            start: { line: i, character: idx },
            end: { line: i, character: idx + 6 },
          },
          severity: 1,
          source: 'mock-lsp',
          code: 'E0425',
          message: 'cannot find value `BROKEN` in this scope',
        });
      }
    }
    send({
      jsonrpc: '2.0',
      method: 'textDocument/publishDiagnostics',
      params: { uri, diagnostics },
    });
    return;
  }
  if (msg.method === 'textDocument/hover') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        contents: { kind: 'markdown', value: '`fn hello_world()`' },
        range: {
          start: { line: msg.params.position.line, character: 0 },
          end: { line: msg.params.position.line, character: 11 },
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/definition') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: [
        {
          uri: msg.params.textDocument.uri,
          range: {
            start: { line: 0, character: 4 },
            end: { line: 0, character: 15 },
          },
        },
      ],
    });
    return;
  }
  if (msg.method === 'shutdown') {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
    return;
  }
  if (msg.method === 'exit') {
    process.exit(0);
  }
}

process.stdin.on('data', (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf('\r\n\r\n');
    if (headerEnd === -1) break;
    const header = buffer.slice(0, headerEnd).toString('utf8');
    const match = header.match(/Content-Length:\s*(\d+)/i);
    if (!match) break;
    const length = Number(match[1]);
    const total = headerEnd + 4 + length;
    if (buffer.length < total) break;
    const body = buffer.slice(headerEnd + 4, total).toString('utf8');
    buffer = buffer.slice(total);
    handleMessage(JSON.parse(body));
  }
});
"#;
        std::fs::write(&script_path, script).unwrap();
        script_path
    }

    fn invoke_json(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> std::result::Result<Value, Value> {
        let ipc_url: tauri::Url = if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap();
        tauri::test::get_ipc_response(
            webview,
            tauri::webview::InvokeRequest {
                cmd: cmd.to_string(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: ipc_url,
                body: tauri::ipc::InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap_or(Value::Null))
    }

    // tests -------------------------------------------------------------------

    mod given_a_default_workspace_with_loose_files {
        use super::*;

        mod when_invoking_tauri_ipc_commands {
            use super::*;

            #[test]
            fn should_warm_up_default_workspace_lsps_and_serve_diagnostics_buffer_over_ipc() {
                // Arrange
                let home = TempDir::new().unwrap();
                let script_path = write_mock_lsp_server(home.path());
                let lgtm_dir = home.path().join(".lgtm");
                let default_ws = lgtm_dir.join("workspace");
                std::fs::create_dir_all(&default_ws).unwrap();
                std::fs::write(
                    lgtm_dir.join("config.toml"),
                    format!(
                        "[languages.python]\nenabled = true\nextensions = [\"py\"]\nroot_markers = [\"pyproject.toml\"]\nmise_tools = {{}}\ncommand = \"node\"\nargs = [\"{}\"]\n\n[languages.typescript]\nenabled = true\nextensions = [\"ts\"]\nroot_markers = [\"tsconfig.json\"]\nmise_tools = {{}}\ncommand = \"node\"\nargs = [\"{}\"]\n",
                        script_path.display(),
                        script_path.display(),
                    ),
                )
                .unwrap();
                std::fs::write(
                    default_ws.join("hello.py"),
                    "def hello_world():\n    BROKEN()\n",
                )
                .unwrap();
                std::fs::write(
                    default_ws.join("hello.ts"),
                    "export function helloWorld() {\n  console.log('ok');\n}\n",
                )
                .unwrap();

                let app = configure_builder(
                    tauri::test::mock_builder(),
                    Some(home.path().to_path_buf()),
                    None,
                )
                .build(app_context())
                .unwrap();
                init_app_state(app.handle(), Some(home.path().to_path_buf()), None).unwrap();
                let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                    .build()
                    .unwrap();

                // Act
                let opened = invoke_json(&webview, "workspace_open", json!({})).unwrap();

                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline {
                    let state = invoke_json(&webview, "lsp_get_state", json!({})).unwrap();
                    let ready_count = state["servers"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|s| s["status"].as_str() == Some("ready"))
                                .count()
                        })
                        .unwrap_or(0);
                    if ready_count >= 2
                        && !state["diagnosticsByPath"].as_object().unwrap().is_empty()
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }

                let read_py = invoke_json(
                    &webview,
                    "workspace_read",
                    json!({ "relativePath": "hello.py" }),
                )
                .unwrap();
                let hover = invoke_json(
                    &webview,
                    "lsp_hover",
                    json!({ "relativePath": "hello.py", "offset": 4 }),
                )
                .unwrap();
                let defs = invoke_json(
                    &webview,
                    "lsp_goto_definition",
                    json!({ "relativePath": "hello.py", "offset": 4 }),
                )
                .unwrap();
                let all_buffer =
                    invoke_json(&webview, "lsp_get_diagnostics_buffer", json!({})).unwrap();
                let py_buffer = invoke_json(
                    &webview,
                    "lsp_get_diagnostics_buffer",
                    json!({ "languageId": "python" }),
                )
                .unwrap();

                app.state::<LspState>().0.shutdown_all();

                // Assert
                assert!(default_ws.join(".lgtm/mise.toml").is_file());
                assert_eq!(opened["entries"].as_array().unwrap().len(), 2);
                assert_eq!(read_py["languageId"].as_str(), Some("python"));
                assert_eq!(read_py["lspStatus"]["status"].as_str(), Some("ready"));
                assert!(
                    hover["contents"]
                        .as_str()
                        .unwrap()
                        .contains("fn hello_world()")
                );
                assert_eq!(defs.as_array().unwrap().len(), 1);
                assert_eq!(all_buffer["title"].as_str(), Some("LSP Diagnostics"));
                assert!(
                    all_buffer["content"]
                        .as_str()
                        .unwrap()
                        .contains("python [ready]")
                );
                assert!(
                    all_buffer["content"]
                        .as_str()
                        .unwrap()
                        .contains("typescript [ready]")
                );
                assert_eq!(
                    py_buffer["title"].as_str(),
                    Some("LSP Diagnostics (python)")
                );
                assert!(
                    py_buffer["content"]
                        .as_str()
                        .unwrap()
                        .contains("cannot find value `BROKEN` in this scope")
                );
            }
        }
    }
}
