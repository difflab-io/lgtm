use lgtm_config::{ConfigService, LgtmConfigError};

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
