mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::write_mock_lsp_server;
use lgtm_config::ConfigService;
use lgtm_lsp::{
    LgtmLspManager, LspError, LspEvent, LspServerStatus, LspSessionKey, LspWorkspaceSnapshot,
};
use serde_json::json;
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

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
        let script_path = write_mock_lsp_server(&workspace_root.join(".lgtm"));
        let global_cfg = workspace_root.join(".lgtm/global.json");
        fs::write(
            workspace_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();

        let default_cfg = serde_json::to_string_pretty(&json!({
            "languages": {
                "rust": {
                    "enabled": true,
                    "extensions": ["rs"],
                    "root_markers": ["Cargo.toml"],
                    "mise_tools": {},
                    "command": "node",
                    "args": [script_path.display().to_string()]
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
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let snapshot = self
                .manager
                .get_workspace_snapshot(&self.workspace_root)
                .unwrap();
            if predicate(&snapshot) {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        self.manager
            .get_workspace_snapshot(&self.workspace_root)
            .unwrap()
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
            let broken_source = "fn helper() {}\nfn main() {\n    BROKEN();\n    UNUSED();\n}\n";
            fs::write(&src_file, broken_source).unwrap();

            // Act: open broken file and wait for initial error + warning diagnostics
            fixture
                .manager
                .prepare_workspace(&fixture.workspace_root)
                .unwrap();
            let (lang, status) = fixture.manager.open_or_sync_file(
                &fixture.workspace_root,
                &src_file,
                broken_source,
            );
            let initial_snapshot =
                fixture.wait_for_snapshot(|snap| !snap.diagnostics_by_path.is_empty());

            // Act: fix the error via did_change + did_save and wait for diagnostics to clear
            let fixed_source = "fn helper() {}\nfn main() {\n    helper();\n}\n";
            fixture
                .manager
                .did_save(&fixture.workspace_root, &src_file, fixed_source)
                .unwrap();
            let cleared_snapshot =
                fixture.wait_for_snapshot(|snap| snap.diagnostics_by_path.is_empty());

            // Act: close document
            fixture
                .manager
                .did_close(&fixture.workspace_root, &src_file)
                .unwrap();

            // Assert
            assert_eq!(lang, Some("rust".to_string()));
            assert_eq!(status.unwrap().status, LspServerStatus::Ready);
            assert_eq!(initial_snapshot.servers[0].error_count, 1);
            assert_eq!(initial_snapshot.servers[0].warning_count, 1);
            assert_eq!(cleared_snapshot.servers[0].error_count, 0);
            assert_eq!(cleared_snapshot.servers[0].warning_count, 0);
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
            let source = "fn helper() {}\nfn main() {\n    helper();\n}\n";
            fs::write(&src_file, source).unwrap();
            fixture
                .manager
                .open_or_sync_file(&fixture.workspace_root, &src_file, source);

            // Act
            let hover = fixture
                .manager
                .get_hover(&fixture.workspace_root, &src_file, 4)
                .unwrap()
                .expect("expected hover result");
            let definitions = fixture
                .manager
                .get_definition(&fixture.workspace_root, &src_file, 31)
                .unwrap();
            let completions = fixture
                .manager
                .get_completions(&fixture.workspace_root, &src_file, 34)
                .unwrap();
            let signature_help = fixture
                .manager
                .get_signature_help(&fixture.workspace_root, &src_file, 37)
                .unwrap()
                .expect("expected signature help");

            // Assert
            assert!(hover.contents.contains("fn helper() -> usize"));
            assert_eq!(hover.from, Some(0));
            assert_eq!(hover.to, Some(6));
            assert_eq!(definitions.len(), 1);
            assert_eq!(definitions[0].path, "lib.rs");
            assert_eq!(definitions[0].offset, 3);
            assert_eq!(definitions[0].end_offset, 9);
            assert_eq!(completions.len(), 1);
            assert_eq!(completions[0].label, "helper");
            assert_eq!(completions[0].kind.as_deref(), Some("function"));
            assert_eq!(
                completions[0].detail.as_deref(),
                Some("fn helper() -> usize")
            );
            assert_eq!(
                completions[0].documentation.as_deref(),
                Some("Computes the helper value.")
            );
            assert_eq!(completions[0].insert_text.as_deref(), Some("helper()"));
            assert_eq!(completions[0].from, Some(31));
            assert_eq!(completions[0].to, Some(34));
            assert_eq!(signature_help.active_signature, 0);
            assert_eq!(signature_help.active_parameter, 1);
            assert_eq!(signature_help.signatures.len(), 1);
            assert_eq!(
                signature_help.signatures[0].label,
                "fn helper(count: usize, label: &str) -> usize"
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
            let source = "fn helper() {}\nfn main() {\n    BROKEN();\n}\n";
            fs::write(&src_file, source).unwrap();
            fixture
                .manager
                .open_or_sync_file(&fixture.workspace_root, &src_file, source);
            let _ = fixture.wait_for_snapshot(|snap| !snap.diagnostics_by_path.is_empty());

            // Act
            let restarted = fixture
                .manager
                .restart_language_server(&fixture.workspace_root, "rust")
                .unwrap();
            let hover_after_restart = fixture
                .manager
                .get_hover(&fixture.workspace_root, &src_file, 4)
                .unwrap();

            // Assert
            assert_eq!(restarted.status, LspServerStatus::Ready);
            assert!(hover_after_restart.is_some());
        }
    }

    mod when_acquiring_an_individual_lsp_client_handle {
        use super::*;

        #[test]
        fn should_return_reusable_handle_that_executes_requests_and_notifications() {
            // Arrange
            let fixture = ManagerFixture::new(None);
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
                    "textDocument/hover",
                    json!({
                        "textDocument": { "uri": "file:///tmp/lib.rs" },
                        "position": { "line": 0, "character": 0 },
                    }),
                )
                .unwrap();

            // Assert
            assert_eq!(handle.get_key(), &key);
            assert!(response.get("contents").is_some());
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
            // Arrange
            let temp = TempDir::new().unwrap();
            let workspace_root = fs::canonicalize(temp.path()).unwrap();
            fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            let script_path = write_mock_lsp_server(&workspace_root.join(".lgtm"));
            let timeout_cfg = serde_json::to_string_pretty(&json!({
                "lsp": {
                    "enabled": true,
                    "auto_install": false,
                    "request_timeout_ms": 250
                },
                "languages": {
                    "rust": {
                        "enabled": true,
                        "extensions": ["rs"],
                        "root_markers": ["Cargo.toml"],
                        "mise_tools": {},
                        "command": "node",
                        "args": [script_path.display().to_string()],
                        "env": { "MOCK_LSP_DELAY_MS": "2000" }
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
