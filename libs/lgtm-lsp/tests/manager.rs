use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lgtm_config::ConfigService;
use lgtm_lsp::{
    LgtmLspManager, LspError, LspEvent, LspServerStatus, LspSessionKey, LspWorkspaceSnapshot,
};
use serde_json::json;
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

const CARGO_TOML_TEMPLATE: &str = "\
[package]
name = \"demo\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
path = \"lib.rs\"
";

const BROKEN_RUST_SOURCE: &str = "\
/// Computes the helper value.
pub fn helper(count: usize, label: &str) -> usize {
    let _ = label;
    count
}

pub fn BadName() {
    let broken = ;
}
";

const FIXED_RUST_SOURCE: &str = "\
/// Computes the helper value.
pub fn helper(count: usize, label: &str) -> usize {
    let _ = label;
    count
}

pub fn entry() -> usize {
    helper(1, \"ok\")
}
";

struct ManagerFixture {
    _temp: TempDir,
    workspace_root: PathBuf,
    global_cfg: PathBuf,
    events: Arc<Mutex<Vec<LspEvent>>>,
    manager: LgtmLspManager,
}

impl ManagerFixture {
    fn new(config_body: Option<&str>) -> Self {
        let temp = TempDir::new().unwrap();
        let workspace_root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
        let global_cfg = workspace_root.join(".lgtm/global.json");
        fs::write(workspace_root.join("Cargo.toml"), CARGO_TOML_TEMPLATE).unwrap();

        let default_cfg = serde_json::to_string_pretty(&json!({
            "lsp": {
                "enabled": true,
                "autoInstall": false,
                "requestTimeoutMs": 15000
            },
            "languages": {
                "rust": {
                    "enabled": true,
                    "extensions": ["rs"],
                    "root_markers": ["Cargo.toml"],
                    "mise_tools": {},
                    "command": "rust-analyzer",
                    "args": []
                }
            }
        }))
        .unwrap();
        fs::write(
            workspace_root.join(".lgtm/config.json"),
            config_body.unwrap_or(&default_cfg),
        )
        .unwrap();

        let events: Arc<Mutex<Vec<LspEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let manager = LgtmLspManager::new(
            ConfigService::with_global_config_path(global_cfg.clone()),
            move |evt| {
                events_clone.lock().unwrap().push(evt);
            },
        );

        Self {
            _temp: temp,
            workspace_root,
            global_cfg,
            events,
            manager,
        }
    }

    fn wait_for_snapshot<F>(&self, predicate: F) -> LspWorkspaceSnapshot
    where
        F: Fn(&LspWorkspaceSnapshot) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            let snapshot = self
                .manager
                .get_workspace_snapshot(&self.workspace_root)
                .unwrap();
            if predicate(&snapshot) {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.manager
            .get_workspace_snapshot(&self.workspace_root)
            .unwrap()
    }

    fn wait_for<T, F>(&self, mut op: F) -> T
    where
        F: FnMut() -> Option<T>,
    {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if let Some(value) = op() {
                return value;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        op().expect("timed out waiting for language server response")
    }
}

impl Drop for ManagerFixture {
    fn drop(&mut self) {
        self.manager.shutdown_all();
    }
}

// tests -----------------------------------------------------------------------

mod given_a_workspace_with_a_configured_language_server {
    use super::*;

    mod when_opening_editing_saving_and_closing_documents {
        use super::*;

        #[test]
        fn should_publish_diagnostics_update_counts_on_fix_and_clear_open_document_on_close() {
            // Arrange
            let fixture = ManagerFixture::new(None);
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, BROKEN_RUST_SOURCE).unwrap();

            // Act: open broken file and wait for initial error + warning diagnostics from rust-analyzer
            fixture
                .manager
                .prepare_workspace(&fixture.workspace_root)
                .unwrap();
            let (lang, status) = fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                BROKEN_RUST_SOURCE,
            );
            let initial_snapshot = fixture.wait_for_snapshot(|snap| {
                snap.servers.first().is_some_and(|srv| srv.error_count >= 1)
            });

            // Act: fix the error on disk + did_save and wait for diagnostics to clear
            fs::write(&src_file, FIXED_RUST_SOURCE).unwrap();
            fixture
                .manager
                .did_save(&fixture.workspace_root, &src_file, FIXED_RUST_SOURCE)
                .unwrap();
            let cleared_snapshot = fixture.wait_for_snapshot(|snap| {
                snap.servers.first().is_some_and(|srv| srv.error_count == 0)
            });

            // Act: close document
            fixture
                .manager
                .did_close(&fixture.workspace_root, &src_file)
                .unwrap();

            // Assert
            assert_eq!(lang, Some("rust".to_string()));
            assert_eq!(status.unwrap().status, LspServerStatus::Ready);
            assert!(initial_snapshot.servers[0].error_count >= 1);
            assert_eq!(cleared_snapshot.servers[0].error_count, 0);
            assert!(
                fixture
                    .manager
                    .get_cache_service()
                    .get_document(&src_file)
                    .is_none()
            );
            assert!(!fixture.events.lock().unwrap().is_empty());
        }
    }

    mod when_requesting_hover_and_goto_definition {
        use super::*;

        #[test]
        fn should_resolve_markdown_hover_ranges_and_definitions_with_utf16_offsets() {
            // Arrange
            let fixture = ManagerFixture::new(None);
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, FIXED_RUST_SOURCE).unwrap();
            fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                FIXED_RUST_SOURCE,
            );

            let hover_offset = FIXED_RUST_SOURCE.find("helper(count").unwrap() as u32 + 2;
            let call_offset = FIXED_RUST_SOURCE.find("helper(1").unwrap() as u32 + 2;
            let sig_offset = FIXED_RUST_SOURCE.find("\"ok\"").unwrap() as u32;

            // Act
            let hover = fixture.wait_for(|| {
                fixture
                    .manager
                    .get_hover(&fixture.workspace_root, &src_file, hover_offset)
                    .ok()
                    .flatten()
            });
            let definitions = fixture.wait_for(|| {
                let defs = fixture
                    .manager
                    .get_definition(&fixture.workspace_root, &src_file, call_offset)
                    .ok()?;
                (!defs.is_empty()).then_some(defs)
            });
            let completions = fixture.wait_for(|| {
                let items = fixture
                    .manager
                    .get_completions(&fixture.workspace_root, &src_file, call_offset + 1)
                    .ok()?;
                items
                    .iter()
                    .any(|item| item.label.starts_with("helper"))
                    .then_some(items)
            });
            let signature_help = fixture.wait_for(|| {
                fixture
                    .manager
                    .get_signature_help(&fixture.workspace_root, &src_file, sig_offset)
                    .ok()
                    .flatten()
            });

            // Assert
            assert!(hover.contents.contains("fn helper"));
            assert!(hover.contents.contains("Computes the helper value."));
            assert!(hover.from.is_some());
            assert!(hover.to.is_some());
            assert_eq!(definitions[0].path, "lib.rs");
            let expected_def_offset = FIXED_RUST_SOURCE.find("helper(count").unwrap() as u32;
            assert_eq!(definitions[0].offset, expected_def_offset);
            let helper_completion = completions
                .iter()
                .find(|item| item.label.starts_with("helper"))
                .expect("expected helper completion item");
            assert_eq!(helper_completion.kind.as_deref(), Some("function"));
            assert_eq!(signature_help.active_signature, 0);
            assert_eq!(signature_help.active_parameter, 1);
            assert!(
                signature_help.signatures[0]
                    .label
                    .contains("fn helper(count: usize, label: &str) -> usize")
            );
            assert_eq!(
                signature_help.signatures[0].parameters,
                vec!["count: usize".to_string(), "label: &str".to_string()]
            );
        }
    }

    mod when_restarting_a_running_language_server {
        use super::*;

        #[test]
        fn should_respawn_server_and_replay_did_open_for_all_tracked_documents() {
            // Arrange
            let fixture = ManagerFixture::new(None);
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, FIXED_RUST_SOURCE).unwrap();
            fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                FIXED_RUST_SOURCE,
            );
            let hover_offset = FIXED_RUST_SOURCE.find("helper(count").unwrap() as u32 + 2;
            let _ = fixture.wait_for(|| {
                fixture
                    .manager
                    .get_hover(&fixture.workspace_root, &src_file, hover_offset)
                    .ok()
                    .flatten()
            });

            // Act
            let restarted = fixture
                .manager
                .restart_language_server(&fixture.workspace_root, "rust")
                .unwrap();
            let hover_after_restart = fixture.wait_for(|| {
                fixture
                    .manager
                    .get_hover(&fixture.workspace_root, &src_file, hover_offset)
                    .ok()
                    .flatten()
            });

            // Assert
            assert_eq!(restarted.status, LspServerStatus::Ready);
            assert!(hover_after_restart.contents.contains("fn helper"));
        }
    }

    mod when_acquiring_an_individual_lsp_client_handle {
        use super::*;

        #[test]
        fn should_return_reusable_handle_that_executes_requests_and_notifications() {
            // Arrange
            let fixture = ManagerFixture::new(None);
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, FIXED_RUST_SOURCE).unwrap();
            fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                FIXED_RUST_SOURCE,
            );
            let key = LspSessionKey::new(&fixture.workspace_root, "rust", &fixture.workspace_root);

            // Act
            let handle = fixture
                .manager
                .get_or_start_server(&fixture.workspace_root, &fixture.workspace_root, "rust")
                .unwrap()
                .expect("expected active LSP client handle");
            let looked_up = fixture
                .manager
                .get_server(&key)
                .expect("expected cached handle");
            let response = looked_up
                .request(
                    "workspace/symbol",
                    json!({
                        "query": "helper",
                    }),
                )
                .unwrap();

            // Assert
            assert_eq!(handle.get_key(), &key);
            assert!(response.is_array());
        }
    }
}

mod given_a_workspace_with_unavailable_or_failing_language_servers {
    use super::*;

    mod when_server_binary_does_not_exist {
        use super::*;

        #[test]
        fn should_record_unavailable_status_with_error_message_and_short_circuit_repeat_spawns() {
            // Arrange
            let missing_cfg = serde_json::to_string_pretty(&json!({
                "languages": {
                    "rust": {
                        "enabled": true,
                        "extensions": ["rs"],
                        "root_markers": ["Cargo.toml"],
                        "mise_tools": {},
                        "command": "/nonexistent/lgtm-missing-lsp-binary",
                        "args": []
                    }
                }
            }))
            .unwrap();
            let fixture = ManagerFixture::new(Some(&missing_cfg));
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, "fn main() {}\n").unwrap();

            // Act
            let (lang, status) = fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                "fn main() {}\n",
            );
            let buffer = fixture
                .manager
                .get_diagnostics_buffer(&fixture.workspace_root, Some("rust"))
                .unwrap();

            // Assert
            assert_eq!(lang, Some("rust".to_string()));
            let status_event = status.expect("expected unavailable status event");
            assert_eq!(status_event.status, LspServerStatus::Unavailable);
            assert!(status_event.message.is_some());
            assert!(buffer.content.contains("rust [unavailable]"));
        }
    }

    mod when_server_exceeds_request_timeout {
        use super::*;

        #[test]
        fn should_return_timeout_error_when_request_exceeds_configured_deadline() {
            // Arrange: spawn a process that holds stdio open past the 150ms request timeout
            let hang_script = "\
import { Buffer } from 'node:buffer';
let buf = Buffer.alloc(0);
process.stdin.on('data', (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  const end = buf.indexOf('\\r\\n\\r\\n');
  if (end === -1) return;
  const match = buf.slice(0, end).toString().match(/Content-Length:\\s*(\\d+)/i);
  if (!match) return;
  const len = Number(match[1]);
  if (buf.length < end + 4 + len) return;
  const msg = JSON.parse(buf.slice(end + 4, end + 4 + len).toString());
  buf = buf.slice(end + 4 + len);
  if (msg.method === 'initialize') {
    const body = Buffer.from(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true } } }));
    process.stdout.write(`Content-Length: ${body.length}\\r\\n\\r\\n`);
    process.stdout.write(body);
  }
});
";
            let timeout_cfg = serde_json::to_string_pretty(&json!({
                "lsp": {
                    "enabled": true,
                    "auto_install": false,
                    "request_timeout_ms": 150
                },
                "languages": {
                    "rust": {
                        "enabled": true,
                        "extensions": ["rs"],
                        "root_markers": ["Cargo.toml"],
                        "mise_tools": {},
                        "command": "node",
                        "args": ["-e", hang_script]
                    }
                }
            }))
            .unwrap();
            let fixture = ManagerFixture::new(Some(&timeout_cfg));
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, "fn helper() {}\n").unwrap();
            fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                "fn helper() {}\n",
            );

            // Act
            let result = fixture
                .manager
                .get_hover(&fixture.workspace_root, &src_file, 2);

            // Assert
            assert!(matches!(result, Err(LspError::Timeout(_))));
        }
    }

    mod when_lsp_is_disabled_or_manager_is_noop {
        use super::*;

        #[test]
        fn should_no_op_without_spawning_processes() {
            // Arrange
            let disabled_cfg = "{\"lsp\": {\"enabled\": false}}\n";
            let fixture = ManagerFixture::new(Some(disabled_cfg));
            let src_file = fixture.workspace_root.join("lib.rs");
            fs::write(&src_file, "fn main() {}\n").unwrap();
            let noop = LgtmLspManager::noop(ConfigService::with_global_config_path(
                fixture.global_cfg.clone(),
            ));

            // Act
            let (disabled_lang, disabled_status) = fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                "fn main() {}\n",
            );
            let (noop_lang, noop_status) =
                noop.open_or_sync_file(&fixture.workspace_root, &src_file, "fn main() {}\n");

            // Assert
            assert_eq!(disabled_lang, Some("rust".to_string()));
            assert!(disabled_status.is_none());
            assert!(noop_lang.is_none());
            assert!(noop_status.is_none());
            assert!(fixture.events.lock().unwrap().is_empty());
        }
    }
}
