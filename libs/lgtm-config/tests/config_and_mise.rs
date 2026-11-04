use lgtm_config::{ConfigService, LgtmConfigError, MiseService};

// tests -----------------------------------------------------------------------

mod given_a_workspace_without_an_existing_mise_file {
    use super::*;

    mod when_ensuring_workspace_layout {
        use super::*;

        #[test]
        fn should_create_managed_mise_file_from_detected_languages_and_config() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            std::fs::write(
                workspace_root.join("Cargo.toml"),
                "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
            )
            .unwrap();
            let config_service = ConfigService::from_home(home);

            // Act
            let managed_path = config_service
                .ensure_workspace_layout(&workspace_root)
                .unwrap();
            let generated = std::fs::read_to_string(&managed_path).unwrap();

            // Assert
            assert_eq!(managed_path, workspace_root.join(".lgtm/mise.toml"));
            assert!(!workspace_root.join("mise.toml").exists());
            assert!(generated.contains("rust-analyzer = \"latest\""));
            assert!(generated.contains("[tasks.\"rust:check\"]"));
            assert!(generated.contains("run = \"cargo check\""));
        }
    }
}

mod given_a_workspace_with_an_existing_managed_mise_file {
    use super::*;

    mod when_project_config_changes {
        use super::*;

        #[test]
        fn should_overwrite_stale_managed_mise_file_with_updated_tools_and_tasks() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
            std::fs::write(
                workspace_root.join("Cargo.toml"),
                "[package]\nname = \"sample\"\n",
            )
            .unwrap();
            let stale_mise_path = workspace_root.join(".lgtm/mise.toml");
            std::fs::write(
                &stale_mise_path,
                "# stale file\n[tools]\nrust-analyzer = \"0.0.1\"\n",
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.toml"),
                "[mise.tools]\nrust-analyzer = \"2026-01-01\"\n\n[mise.tasks.check]\ndescription = \"Custom clippy check\"\nrun = \"cargo clippy --all-targets\"\n",
            )
            .unwrap();
            let config_service = ConfigService::from_home(home);

            // Act
            let updated_path = config_service
                .ensure_workspace_layout(&workspace_root)
                .unwrap();
            let updated = std::fs::read_to_string(&updated_path).unwrap();

            // Assert
            assert_eq!(updated_path, stale_mise_path);
            assert!(updated.contains("rust-analyzer = \"2026-01-01\""));
            assert!(updated.contains("cargo clippy --all-targets"));
            assert!(!updated.contains("0.0.1"));
        }
    }
}

mod given_a_workspace_with_a_user_provided_root_mise_file {
    use super::*;

    mod when_synchronizing_lgtm_managed_mise_configuration {
        use super::*;

        #[test]
        fn should_preserve_user_root_mise_file_and_isolate_generated_config_under_dot_lgtm() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();

            let user_mise_path = workspace_root.join("mise.toml");
            let user_mise_content =
                "[tools]\nbun = \"1.2.0\"\n\n[tasks.build]\nrun = \"bun run build\"\n";
            std::fs::write(&user_mise_path, user_mise_content).unwrap();
            std::fs::write(workspace_root.join("tsconfig.json"), "{}\n").unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/config.toml"),
                "[mise.tasks.check]\ndescription = \"Custom TS check\"\nrun = \"tsc -p tsconfig.json --noEmit\"\n",
            )
            .unwrap();

            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::new(config_service.clone());

            // Act
            let tasks = mise_service.get_tasks(&workspace_root).unwrap();
            let user_mise_after = std::fs::read_to_string(&user_mise_path).unwrap();
            let managed_mise_after =
                std::fs::read_to_string(workspace_root.join(".lgtm/mise.toml")).unwrap();

            // Assert
            assert_eq!(user_mise_after, user_mise_content);
            assert!(managed_mise_after.contains("npm:typescript-language-server"));
            let check_task = tasks.iter().find(|t| t.name == "check").unwrap();
            assert_eq!(check_task.run, "tsc -p tsconfig.json --noEmit");
            let preset_task = tasks.iter().find(|t| t.name == "typescript:check").unwrap();
            assert_eq!(preset_task.run, "tsc --noEmit");
        }
    }
}

mod given_malformed_global_or_project_config_files {
    use super::*;

    mod when_loading_merged_configuration {
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
            std::fs::write(home.join(".lgtm/config.toml"), "[lsp\ninvalid").unwrap();
            let global_err = service.get_config(&workspace_root).unwrap_err();

            std::fs::write(home.join(".lgtm/config.toml"), "[lsp]\nenabled = true\n").unwrap();
            std::fs::write(workspace_root.join(".lgtm/config.toml"), "[mise\ninvalid").unwrap();
            let project_err = service.get_config(&workspace_root).unwrap_err();

            // Assert
            assert!(matches!(
                global_err,
                LgtmConfigError::GlobalConfigParse { .. }
            ));
            assert!(matches!(
                project_err,
                LgtmConfigError::ProjectConfigParse { .. }
            ));
        }
    }
}
