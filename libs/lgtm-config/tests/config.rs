use lgtm_config::{
    ConfigService, DEFAULT_SCHEMA_URL, LgtmConfigError, LineWrap, ThemeMode, UserSettingsPatch,
};

// tests -----------------------------------------------------------------------

mod given_a_git_backed_workspace_without_an_existing_dot_lgtm_directory {
    use super::*;

    mod when_ensuring_workspace_dir {
        use super::*;

        #[test]
        fn should_create_dot_lgtm_directory_and_register_git_exclude_without_touching_root_files() {
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
}

mod given_malformed_global_or_project_config_files {
    use super::*;

    mod when_loading_merged_configuration_or_settings_snapshot {
        use super::*;

        #[test]
        fn should_distinguish_global_config_parse_errors_from_project_config_parse_errors() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            let service = ConfigService::from_home(home.clone());

            // Act
            std::fs::write(
                home.join(".lgtm/config.json"),
                "{ \"appearance\": invalid }",
            )
            .unwrap();
            let global_err = service.get_config(&workspace_root).unwrap_err();
            let global_settings_err = service
                .get_settings_snapshot(Some(&workspace_root))
                .unwrap_err();

            std::fs::write(
                home.join(".lgtm/config.json"),
                "{\n  \"appearance\": { \"theme\": \"dark\" }\n}\n",
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                "{\n  \"appearance\": {\n    \"unknownNestedField\": true\n  }\n}\n",
            )
            .unwrap();
            let project_err = service.get_config(&workspace_root).unwrap_err();
            let project_settings_err = service
                .get_settings_snapshot(Some(&workspace_root))
                .unwrap_err();

            // Assert
            assert!(matches!(
                global_err,
                LgtmConfigError::GlobalConfigParse { .. }
            ));
            assert!(matches!(
                global_settings_err,
                LgtmConfigError::GlobalConfigParse { .. }
            ));
            assert!(matches!(
                project_err,
                LgtmConfigError::ProjectConfigParse { .. }
            ));
            assert!(matches!(
                project_settings_err,
                LgtmConfigError::ProjectConfigParse { .. }
            ));
        }
    }
}

mod given_global_and_project_nested_json_configs {
    use super::*;

    mod when_resolving_settings {
        use super::*;

        #[test]
        fn should_recursively_merge_project_leaf_overrides_over_global_settings_and_defaults() {
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
  }
}
"#,
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                "{\n  \"$schema\": \"https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json\",\n  \"appearance\": {\n    \"theme\": \"light\"\n  }\n}\n",
            )
            .unwrap();
            let service = ConfigService::from_home(home);

            // Act
            let snapshot = service
                .get_settings_snapshot(Some(&workspace_root))
                .unwrap();

            // Assert
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
        }
    }
}

mod given_schema_generation {
    use super::*;

    mod when_comparing_with_checked_in_schema_file {
        use super::*;

        #[test]
        fn should_match_schemas_config_schema_json_at_repository_root() {
            // Arrange
            let repo_schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .expect("expected workspace root")
                .join("schemas/config.schema.json");

            // Act
            let generated = UserSettingsPatch::json_schema_pretty();
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
