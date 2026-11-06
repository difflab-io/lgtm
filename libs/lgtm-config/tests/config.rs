use lgtm_config::{
    AppearanceSettingsPatch, ConfigFormat, ConfigScope, ConfigService, EditorSettingsPatch,
    LgtmConfigError, ThemeMode, UserSettingsPatch, WorkspaceSettingsPatch,
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
                workspace_root.join(".lgtm/config.yaml"),
                "appearance:\n  unknownNestedField: true\n",
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

mod given_global_and_project_nested_json_or_yaml_configs {
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
  "$schema": "./schemas/config.schema.json",
  "appearance": {
    "theme": "dark"
  },
  "workspace": {
    "activeRoot": "/tmp/ws-global",
    "recentRoots": ["/tmp/ws-global"]
  },
  "editor": {
    "lineWrapping": true
  }
}
"#,
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.yaml"),
                "# yaml-language-server: $schema=./schemas/config.schema.json\nappearance:\n  theme: light\n",
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
            assert!(snapshot.effective.editor.line_wrapping);
        }
    }

    mod when_patching_nested_settings_without_explicit_scope {
        use super::*;

        #[test]
        fn should_route_each_leaf_to_project_config_only_when_already_overridden_there() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(home.join(".lgtm")).unwrap();
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            let service = ConfigService::from_home(home.clone());

            // 1. No project config exists yet -> theme update goes to global config.json.
            let first = service
                .update_settings(
                    Some(&workspace_root),
                    None,
                    UserSettingsPatch {
                        schema: None,
                        appearance: Some(AppearanceSettingsPatch {
                            theme: Some(ThemeMode::Dark),
                        }),
                        workspace: None,
                        editor: None,
                    },
                )
                .unwrap();
            assert_eq!(first.effective.appearance.theme, ThemeMode::Dark);
            assert!(home.join(".lgtm/config.json").is_file());
            assert!(!workspace_root.join(".lgtm/config.json").exists());

            // 2. Seed a project override for `appearance.theme = "light"` only.
            std::fs::write(
                workspace_root.join(".lgtm/config.json"),
                "{\n  \"appearance\": {\n    \"theme\": \"light\"\n  }\n}\n",
            )
            .unwrap();

            // Act: patch both `appearance.theme` (overridden in project) and `editor.lineWrapping` + `workspace.activeRoot` (not in project).
            let updated = service
                .update_settings(
                    Some(&workspace_root),
                    None,
                    UserSettingsPatch {
                        schema: None,
                        appearance: Some(AppearanceSettingsPatch {
                            theme: Some(ThemeMode::System),
                        }),
                        workspace: Some(WorkspaceSettingsPatch {
                            active_root: Some("/tmp/ws-next".to_string()),
                            recent_roots: Some(vec!["/tmp/ws-next".to_string()]),
                        }),
                        editor: Some(EditorSettingsPatch {
                            line_wrapping: Some(false),
                        }),
                    },
                )
                .unwrap();

            // Assert
            assert_eq!(updated.effective.appearance.theme, ThemeMode::System);
            assert!(!updated.effective.editor.line_wrapping);
            assert_eq!(
                updated.global.appearance.and_then(|a| a.theme),
                Some(ThemeMode::Dark)
            );
            assert_eq!(
                updated.global.editor.and_then(|e| e.line_wrapping),
                Some(false)
            );
            assert_eq!(
                updated.global.workspace.and_then(|w| w.active_root),
                Some("/tmp/ws-next".to_string())
            );
            let proj = updated.project.expect("expected project patch");
            assert_eq!(
                proj.appearance.and_then(|a| a.theme),
                Some(ThemeMode::System)
            );
            assert!(proj.editor.is_none());
            assert!(proj.workspace.is_none());
        }
    }

    mod when_opening_and_writing_config_documents {
        use super::*;

        #[test]
        fn should_initialize_default_document_and_parse_updated_content_into_snapshot() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            let service = ConfigService::from_home(home.clone());

            // Act
            let opened = service
                .open_config_document(Some(&workspace_root), ConfigScope::Global, None)
                .unwrap();
            let (written_doc, snapshot) = service
                .write_config_document(
                    Some(&workspace_root),
                    ConfigScope::Global,
                    "{\n  \"$schema\": \"./schemas/config.schema.json\",\n  \"appearance\": {\n    \"theme\": \"light\"\n  },\n  \"editor\": {\n    \"lineWrapping\": false\n  }\n}\n",
                    Some(ConfigFormat::Json),
                )
                .unwrap();

            // Assert
            assert_eq!(opened.scope, ConfigScope::Global);
            assert_eq!(opened.format, ConfigFormat::Json);
            assert_eq!(opened.title, "Settings (config.json)");
            assert_eq!(opened.language_id, "json");
            assert!(opened.content.contains("\"$schema\""));
            assert_ne!(opened.revision, written_doc.revision);
            let resolved = snapshot.expect("expected valid snapshot after write");
            assert_eq!(resolved.effective.appearance.theme, ThemeMode::Light);
            assert!(!resolved.effective.editor.line_wrapping);
        }
    }
}

mod given_schema_generation {
    use super::*;

    mod when_ensuring_schema_files {
        use super::*;

        #[test]
        fn should_write_nested_config_schema_json_and_biome_json() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            let service = ConfigService::from_home(home.clone());
            service.ensure_workspace_dir(&workspace_root).unwrap();

            // Act
            let global_schema_path = service.ensure_schema_files(Some(&workspace_root)).unwrap();
            let project_schema_path = ConfigService::get_project_schema_path(&workspace_root);
            let schema_raw = std::fs::read_to_string(&global_schema_path).unwrap();
            let schema_val: serde_json::Value = serde_json::from_str(&schema_raw).unwrap();

            // Assert
            assert!(global_schema_path.is_file());
            assert!(project_schema_path.is_file());
            assert!(home.join(".lgtm/biome.json").is_file());
            let props = schema_val
                .get("properties")
                .and_then(|p| p.as_object())
                .expect("expected properties object in schema");
            assert!(props.contains_key("$schema"));
            assert!(props.contains_key("appearance"));
            assert!(props.contains_key("workspace"));
            assert!(props.contains_key("editor"));
        }
    }
}
