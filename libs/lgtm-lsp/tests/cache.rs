use std::fs;

use lgtm_config::ConfigService;
use lgtm_lsp::{
    LspCacheService, LspDiagnostic, LspDiagnosticSeverity, LspDocumentSyncAction, LspServerStatus,
    LspServerStatusEvent, LspSessionKey,
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

fn sample_diagnostic(severity: LspDiagnosticSeverity, code: &str, message: &str) -> LspDiagnostic {
    LspDiagnostic {
        from: 4,
        to: 10,
        line: 1,
        character: 4,
        end_line: 1,
        end_character: 10,
        severity,
        message: message.to_string(),
        source: Some("rustc".to_string()),
        code: Some(code.to_string()),
    }
}

// tests -----------------------------------------------------------------------

mod given_an_lsp_cache_service {
    use super::*;

    mod when_persisting_and_loading_workspace_discovery_cache {
        use super::*;

        #[test]
        fn should_round_trip_discovered_languages_and_merged_config_on_disk_and_in_memory() {
            // Arrange
            let temp = TempDir::new().unwrap();
            let workspace_root = fs::canonicalize(temp.path()).unwrap();
            let config_service =
                ConfigService::with_global_config_path(workspace_root.join(".lgtm/global.toml"));
            let config = config_service.get_config(&workspace_root).unwrap();
            let cache = LspCacheService::new();
            let languages = vec!["rust".to_string(), "typescript".to_string()];

            // Act
            let written = cache
                .write_discovery_cache(&workspace_root, &languages, &config)
                .unwrap();
            let loaded_disk = cache
                .get_discovery_cache(&workspace_root)
                .unwrap()
                .expect("expected persisted discovery cache");
            let loaded_mem = cache
                .get_or_load_workspace_config(&config_service, &workspace_root)
                .unwrap();

            // Assert
            assert_eq!(written, loaded_disk);
            assert_eq!(loaded_disk.detected_languages, languages);
            assert_eq!(loaded_mem, config);
        }
    }

    mod when_tracking_open_document_lifecycle {
        use super::*;

        #[test]
        fn should_open_version_1_skip_identical_edits_increment_on_changes_and_remove_on_close() {
            // Arrange
            let temp = TempDir::new().unwrap();
            let workspace_root = fs::canonicalize(temp.path()).unwrap();
            let file_path = workspace_root.join("lib.rs");
            fs::write(&file_path, "pub fn v1() {}\n").unwrap();
            let cache = LspCacheService::new();

            // Act
            let first = cache.sync_document(
                &workspace_root,
                &workspace_root,
                &file_path,
                "rust",
                "pub fn v1() {}\n",
            );
            let duplicate = cache.sync_document(
                &workspace_root,
                &workspace_root,
                &file_path,
                "rust",
                "pub fn v1() {}\n",
            );
            let edited = cache
                .update_open_document(&file_path, "pub fn v2() {}\n")
                .expect("expected open document");
            let rust_docs = cache.get_documents_for_language(&workspace_root, "rust");
            let removed = cache
                .remove_document(&file_path)
                .expect("expected removed document");
            let after_close = cache.get_document(&file_path);

            // Assert
            assert!(matches!(first, LspDocumentSyncAction::Opened(ref d) if d.version == 1));
            assert!(matches!(duplicate, LspDocumentSyncAction::Unchanged(ref d) if d.version == 1));
            assert!(
                matches!(edited, LspDocumentSyncAction::Changed(ref d) if d.version == 2 && d.text == "pub fn v2() {}\n")
            );
            assert_eq!(rust_docs.len(), 1);
            assert_eq!(removed.version, 2);
            assert!(after_close.is_none());
        }
    }

    mod when_publishing_and_clearing_file_diagnostics {
        use super::*;

        #[test]
        fn should_update_per_language_error_and_warning_counts_and_render_filtered_buffers() {
            // Arrange
            let temp = TempDir::new().unwrap();
            let workspace_root = fs::canonicalize(temp.path()).unwrap();
            let rs_file = workspace_root.join("lib.rs");
            let py_file = workspace_root.join("app.py");
            fs::write(&rs_file, "fn main() {}\n").unwrap();
            fs::write(&py_file, "print(1)\n").unwrap();

            let cache = LspCacheService::new();
            let rust_key = LspSessionKey::new(&workspace_root, "rust", &workspace_root);
            let py_key = LspSessionKey::new(&workspace_root, "python", &workspace_root);

            cache.set_status(
                rust_key.clone(),
                LspServerStatusEvent {
                    workspace_root: workspace_root.to_string_lossy().to_string(),
                    language_id: "rust".to_string(),
                    project_root: workspace_root.to_string_lossy().to_string(),
                    status: LspServerStatus::Ready,
                    command: "rust-analyzer".to_string(),
                    message: None,
                    error_count: 0,
                    warning_count: 0,
                },
            );
            cache.set_status(
                py_key.clone(),
                LspServerStatusEvent {
                    workspace_root: workspace_root.to_string_lossy().to_string(),
                    language_id: "python".to_string(),
                    project_root: workspace_root.to_string_lossy().to_string(),
                    status: LspServerStatus::Ready,
                    command: "pyright-langserver".to_string(),
                    message: None,
                    error_count: 0,
                    warning_count: 0,
                },
            );

            // Act
            let (file_event, updated_rust_status) = cache.set_file_diagnostics(
                &rust_key,
                &rs_file,
                vec![
                    sample_diagnostic(LspDiagnosticSeverity::Error, "E0425", "unresolved symbol"),
                    sample_diagnostic(LspDiagnosticSeverity::Warning, "W0001", "unused variable"),
                ],
            );
            let _ = cache.set_file_diagnostics(
                &py_key,
                &py_file,
                vec![sample_diagnostic(
                    LspDiagnosticSeverity::Warning,
                    "reportUnusedImport",
                    "unused import",
                )],
            );

            let snapshot = cache.get_workspace_snapshot(&workspace_root);
            let rust_buffer = cache.get_diagnostics_buffer(&workspace_root, Some("rust"));
            let all_buffer = cache.get_diagnostics_buffer(&workspace_root, None);

            // Assert
            assert_eq!(file_event.path, "lib.rs");
            let rust_status = updated_rust_status.expect("expected updated rust status");
            assert_eq!(rust_status.error_count, 1);
            assert_eq!(rust_status.warning_count, 1);
            assert_eq!(snapshot.servers.len(), 2);
            assert_eq!(snapshot.diagnostics_by_path.len(), 2);
            assert_eq!(rust_buffer.title, "LSP Diagnostics (rust)");
            assert!(rust_buffer.content.contains("unresolved symbol"));
            assert!(!rust_buffer.content.contains("unused import"));
            assert!(all_buffer.content.contains("unresolved symbol"));
            assert!(all_buffer.content.contains("unused import"));
        }
    }

    mod when_clearing_workspace_or_all_state {
        use super::*;

        #[test]
        fn should_evict_only_targeted_workspace_entries_or_entire_cache() {
            // Arrange
            let ws_a_dir = TempDir::new().unwrap();
            let ws_b_dir = TempDir::new().unwrap();
            let ws_a = fs::canonicalize(ws_a_dir.path()).unwrap();
            let ws_b = fs::canonicalize(ws_b_dir.path()).unwrap();
            let file_a = ws_a.join("a.rs");
            let file_b = ws_b.join("b.rs");
            fs::write(&file_a, "fn a() {}\n").unwrap();
            fs::write(&file_b, "fn b() {}\n").unwrap();

            let cache = LspCacheService::new();
            cache.sync_document(&ws_a, &ws_a, &file_a, "rust", "fn a() {}\n");
            cache.sync_document(&ws_b, &ws_b, &file_b, "rust", "fn b() {}\n");

            // Act
            cache.clear_workspace(&ws_a);
            let doc_a_after_ws_clear = cache.get_document(&file_a);
            let doc_b_after_ws_clear = cache.get_document(&file_b);
            cache.clear_all();
            let doc_b_after_clear_all = cache.get_document(&file_b);

            // Assert
            assert!(doc_a_after_ws_clear.is_none());
            assert!(doc_b_after_ws_clear.is_some());
            assert!(doc_b_after_clear_all.is_none());
        }
    }
}
