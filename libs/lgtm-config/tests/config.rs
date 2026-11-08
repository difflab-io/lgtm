use lgtm_config::{ConfigService, DEFAULT_SCHEMA_URL, LgtmConfig, LineWrap, ThemeMode};

// tests -----------------------------------------------------------------------

mod given_no_global_config {
    use super::*;

    mod when_resolving_config_snapshot {
        use super::*;

        #[test]
        fn should_return_default_config_and_presets() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            let service = ConfigService::from_home(home);

            // Act
            let snapshot = service.get_config_snapshot(Some(&workspace_root)).unwrap();
            let config = service.get_config(&workspace_root).unwrap();

            // Assert
            assert_eq!(snapshot.effective, config);
            assert_eq!(snapshot.effective, LgtmConfig::default());
            assert_eq!(snapshot.effective.appearance.theme, ThemeMode::System);
            assert_eq!(snapshot.effective.editor.line_wrapping, LineWrap::Auto);
            assert!(snapshot.project.is_none());
        }
    }

    mod when_ensuring_workspace_dir {
        use super::*;

        #[test]
        fn should_create_dot_lgtm_directory_and_register_git_exclude() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(workspace_root.join(".git/info")).unwrap();
            std::fs::write(
                workspace_root.join("Cargo.toml"),
                "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
            )
            .unwrap();
            let config_service = ConfigService::from_home(home.clone());

            // Act
            let lgtm_dir = config_service
                .ensure_workspace_dir(&workspace_root)
                .unwrap();
            let exclude_contents =
                std::fs::read_to_string(workspace_root.join(".git/info/exclude")).unwrap();

            // Assert
            assert_eq!(lgtm_dir, workspace_root.join(".lgtm"));
            assert_eq!(config_service.get_lgtm_home_dir(), home.join(".lgtm"));
            assert!(lgtm_dir.is_dir());
            assert!(exclude_contents.lines().any(|line| line.trim() == ".lgtm/"));
        }
    }

    mod when_generating_json_schema {
        use super::*;

        #[test]
        fn should_match_checked_in_schemas_config_schema_json() {
            // Arrange
            let repo_schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .expect("expected workspace root")
                .join("schemas/config.schema.json");

            // Act
            let generated = LgtmConfig::json_schema_pretty();
            if std::env::var_os("LGTM_WRITE_SCHEMA").is_some() {
                if let Some(parent) = repo_schema_path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&repo_schema_path, &generated).unwrap();
            }
            let checked_in = std::fs::read_to_string(&repo_schema_path)
                .expect("expected schemas/config.schema.json at repo root");

            // Assert
            assert_eq!(
                DEFAULT_SCHEMA_URL,
                "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json"
            );
            assert_eq!(checked_in, generated);
        }
    }
}

mod given_global_config_and_no_local_config {
    use super::*;

    mod when_global_config_is_valid {
        use super::*;

        #[test]
        fn should_override_defaults_with_global_settings() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(&workspace_root).unwrap();
            std::fs::write(
                home.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "theme": "dark"
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#,
            )
            .unwrap();
            let service = ConfigService::from_home(home);

            // Act
            let snapshot = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Assert
            assert_eq!(snapshot.effective.appearance.theme, ThemeMode::Dark);
            assert_eq!(snapshot.effective.editor.line_wrapping, LineWrap::Off);
            assert!(snapshot.project.is_none());
        }
    }

    mod when_global_config_has_errors {
        use super::*;

        #[test]
        fn should_ignore_invalid_fields_and_fall_back_to_defaults() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(&workspace_root).unwrap();
            let service = ConfigService::from_home(home.clone());

            // Act: unknown field and invalid enum value alongside a valid editor setting
            std::fs::write(
                home.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "theme": "invalid-theme",
    "unknownGlobalProp": true
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#,
            )
            .unwrap();
            let partial_snapshot = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Act: completely malformed JSON syntax
            std::fs::write(home.join(".lgtm/config.json"), "{ \"appearance\": broken }").unwrap();
            let syntax_fallback = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Assert
            assert_eq!(
                partial_snapshot.effective.appearance.theme,
                ThemeMode::System
            );
            assert_eq!(
                partial_snapshot.effective.editor.line_wrapping,
                LineWrap::Off
            );
            assert_eq!(syntax_fallback.effective, LgtmConfig::default());
        }
    }
}

mod given_global_and_local_configs {
    use super::*;

    mod when_both_configs_are_valid {
        use super::*;

        #[test]
        fn should_recursively_merge_local_overrides_over_global_and_defaults() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            std::fs::write(
                home.join(".lgtm/config.json"),
                r#"{
  "$schema": "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json",
  "appearance": {
    "theme": "dark"
  },
  "workspace": {
    "activeRoot": "/tmp/ws-global",
    "recentRoots": ["/tmp/ws-global"]
  },
  "editor": {
    "lineWrapping": 100
  },
  "lsp": {
    "autoInstall": false,
    "requestTimeoutMs": 7000
  }
}
"#,
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                "{\n  \"$schema\": \"https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json\",\n  \"appearance\": {\n    \"theme\": \"light\"\n  },\n  \"lsp\": {\n    \"requestTimeoutMs\": 9000\n  }\n}\n",
            )
            .unwrap();
            let service = ConfigService::from_home(home);

            // Act
            let snapshot = service.get_config_snapshot(Some(&workspace_root)).unwrap();
            let config = service.get_config(&workspace_root).unwrap();

            // Assert
            assert_eq!(snapshot.effective, config);
            assert_eq!(snapshot.effective.appearance.theme, ThemeMode::Light);
            assert_eq!(
                snapshot.effective.workspace.active_root,
                Some("/tmp/ws-global".to_string())
            );
            assert_eq!(
                snapshot.effective.workspace.recent_roots,
                vec!["/tmp/ws-global".to_string()]
            );
            assert_eq!(
                snapshot.effective.editor.line_wrapping,
                LineWrap::Column(100)
            );
            assert!(!snapshot.effective.lsp.auto_install);
            assert_eq!(snapshot.effective.lsp.request_timeout_ms, 9000);
        }
    }

    mod when_local_config_has_errors {
        use super::*;

        #[test]
        fn should_ignore_invalid_local_fields_and_preserve_valid_local_and_global_settings() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            std::fs::write(
                home.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "theme": "dark"
  },
  "editor": {
    "lineWrapping": "auto"
  }
}
"#,
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "unknownLocalField": true
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#,
            )
            .unwrap();
            let service = ConfigService::from_home(home);

            // Act
            let snapshot = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Assert
            assert_eq!(snapshot.effective.appearance.theme, ThemeMode::Dark);
            assert_eq!(snapshot.effective.editor.line_wrapping, LineWrap::Off);
        }
    }

    mod when_both_global_and_local_configs_have_errors {
        use super::*;

        #[test]
        fn should_preserve_any_valid_fields_and_fall_back_to_default_config() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            let service = ConfigService::from_home(home.clone());

            // Act: both files have invalid properties alongside valid ones
            std::fs::write(
                home.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "theme": "invalid-theme"
  },
  "lsp": {
    "requestTimeoutMs": 8000
  }
}
"#,
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                r#"{
  "appearance": {
    "unknownLocalField": true
  },
  "editor": {
    "lineWrapping": "invalid-wrap"
  }
}
"#,
            )
            .unwrap();
            let partial = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Act: both files are completely corrupted JSON
            std::fs::write(home.join(".lgtm/config.json"), "{ corrupted global").unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                "{ corrupted local",
            )
            .unwrap();
            let total_fallback = service.get_config_snapshot(Some(&workspace_root)).unwrap();

            // Assert
            assert_eq!(partial.effective.appearance.theme, ThemeMode::System);
            assert_eq!(partial.effective.editor.line_wrapping, LineWrap::Auto);
            assert_eq!(partial.effective.lsp.request_timeout_ms, 8000);
            assert_eq!(total_fallback.effective, LgtmConfig::default());
        }
    }
}
