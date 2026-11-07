use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lgtm_config::ConfigService;
use lgtm_env::{LgtmEnvError, MiseService};

// fixtures --------------------------------------------------------------------

fn spawn_installer_http_server(
    status_line: &'static str,
    body: String,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            hits_clone.fetch_add(1, Ordering::SeqCst);
            let mut stream = stream;
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://{addr}/install.sh"), hits)
}

#[cfg(unix)]
fn working_installer_script() -> String {
    r#"#!/bin/sh
set -eu
mkdir -p "$(dirname "$MISE_INSTALL_PATH")"
cat > "$MISE_INSTALL_PATH" << 'EOF'
#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  echo "2026.3.0 macos-arm64 (mock-lgtm-mise)"
  exit 0
fi
if [ "${1:-}" = "trust" ]; then
  exit 0
fi
if [ "${1:-}" = "install" ]; then
  shift
  printf "%s\n" "$@" > "${MISE_CONFIG_FILE}.installed"
  exit 0
fi
if [ "${1:-}" = "run" ]; then
  shift
  if [ "${1:-}" = "--skip-tools" ]; then
    shift
  fi
  task_name="${1:-}"
  shift || true
  printf "ran:%s:%s\n" "$task_name" "$*" > "${MISE_CONFIG_FILE}.ran"
  exit 0
fi
if [ "${1:-}" = "exec" ]; then
  while [ $# -gt 0 ]; do
    if [ "$1" = "--" ]; then
      shift
      exec "$@"
    fi
    shift
  done
fi
exit 0
EOF
chmod +x "$MISE_INSTALL_PATH"
"#
    .to_string()
}

// tests -----------------------------------------------------------------------

mod given_an_isolated_environment_without_mise {
    use super::*;

    mod when_checking_mise_presence {
        use super::*;

        #[test]
        fn should_report_mise_not_installed_and_return_none_for_binary_path() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let config_service = ConfigService::from_home(home.clone());
            let mise_service = MiseService::with_isolated_env(config_service, None);

            // Act
            let is_installed = mise_service.is_mise_installed();
            let found_path = mise_service.find_mise_binary();
            let expected_managed = mise_service.get_managed_mise_binary_path();

            // Assert
            assert!(!is_installed);
            assert!(found_path.is_none());
            assert!(expected_managed.starts_with(home.join(".lgtm").join("bin")));
            assert!(!expected_managed.exists());
        }
    }

    mod when_building_exec_command_without_auto_install {
        use super::*;

        #[test]
        fn should_return_mise_not_installed_error() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::with_isolated_env(config_service, None);
            let mut tools = std::collections::BTreeMap::new();
            tools.insert("rust-analyzer".to_string(), "latest".to_string());

            // Act
            let result = mise_service.build_exec_command(
                &workspace_root,
                &workspace_root,
                &tools,
                "rust-analyzer",
                &[],
                false,
            );

            // Assert
            assert!(matches!(result, Err(LgtmEnvError::MiseNotInstalled)));
        }
    }

    #[cfg(unix)]
    mod when_a_non_executable_or_corrupted_file_exists_at_managed_mise_path {
        use super::*;

        #[test]
        fn should_treat_corrupted_binary_as_missing_and_reinstall_cleanly() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let (installer_url, hits) =
                spawn_installer_http_server("200 OK", working_installer_script());
            let config_service = ConfigService::from_home(home.clone());
            let mise_service = MiseService::with_isolated_env(config_service, Some(installer_url));
            let managed_path = mise_service.get_managed_mise_binary_path();
            std::fs::create_dir_all(managed_path.parent().unwrap()).unwrap();
            std::fs::write(&managed_path, "not-an-executable").unwrap();

            // Act
            let present_with_corrupted_file = mise_service.is_mise_installed();
            let installed_path = mise_service.ensure_mise_binary().unwrap();
            let present_after_repair = mise_service.is_mise_installed();

            // Assert
            assert!(!present_with_corrupted_file);
            assert!(present_after_repair);
            assert_eq!(installed_path, home.join(".lgtm/bin/mise"));
            assert_eq!(hits.load(Ordering::SeqCst), 1);
        }
    }

    #[cfg(unix)]
    mod when_ensuring_mise_binary_with_installer {
        use super::*;

        #[test]
        fn should_download_and_install_mise_into_lgtm_home_bin_and_reuse_on_subsequent_calls() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let (installer_url, hits) =
                spawn_installer_http_server("200 OK", working_installer_script());
            let config_service = ConfigService::from_home(home.clone());
            let mise_service = MiseService::with_isolated_env(config_service, Some(installer_url));

            // Act
            let installed_before = mise_service.is_mise_installed();
            let first_path = mise_service.ensure_mise_binary().unwrap();
            let installed_after = mise_service.is_mise_installed();
            let second_path = mise_service.ensure_mise_binary().unwrap();

            // Assert
            assert!(!installed_before);
            assert!(installed_after);
            assert_eq!(first_path, home.join(".lgtm/bin/mise"));
            assert_eq!(second_path, first_path);
            assert!(first_path.is_file());
            assert_eq!(hits.load(Ordering::SeqCst), 1);
        }
    }

    #[cfg(unix)]
    mod when_installer_returns_non_zero_status {
        use super::*;

        #[test]
        fn should_return_mise_install_failed_error_and_leave_binary_absent() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let failing_script =
                "#!/bin/sh\necho 'simulated network or checksum failure' >&2\nexit 1\n".to_string();
            let (installer_url, _hits) = spawn_installer_http_server("200 OK", failing_script);
            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::with_isolated_env(config_service, Some(installer_url));

            // Act
            let result = mise_service.ensure_mise_binary();

            // Assert
            let err = result.unwrap_err();
            assert!(matches!(err, LgtmEnvError::MiseInstallFailed(_)));
            assert!(
                err.to_string()
                    .contains("simulated network or checksum failure")
            );
            assert!(!mise_service.is_mise_installed());
        }
    }

    #[cfg(unix)]
    mod when_running_tasks_and_installing_tools_without_preinstalled_mise {
        use super::*;

        #[test]
        fn should_bootstrap_mise_automatically_and_execute_task_and_tool_installs() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            std::fs::write(
                workspace_root.join("Cargo.toml"),
                "[package]\nname = \"sample\"\n",
            )
            .unwrap();
            let (installer_url, hits) =
                spawn_installer_http_server("200 OK", working_installer_script());
            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::with_isolated_env(config_service, Some(installer_url));

            // Act
            let exit_code = mise_service
                .run_task(&workspace_root, "check", &["--all-targets".to_string()])
                .unwrap();
            mise_service
                .install_tools(&workspace_root, &[("rust-analyzer", "latest")])
                .unwrap();

            // Assert
            assert_eq!(exit_code, 0);
            assert_eq!(hits.load(Ordering::SeqCst), 1);
            let mise_file = workspace_root.join(".lgtm/mise.toml");
            let ran_marker =
                std::fs::read_to_string(format!("{}.ran", mise_file.display())).unwrap();
            let installed_marker =
                std::fs::read_to_string(format!("{}.installed", mise_file.display())).unwrap();
            assert_eq!(ran_marker.trim(), "ran:check:--all-targets");
            assert_eq!(installed_marker.trim(), "rust-analyzer@latest");
        }
    }
}

mod given_a_workspace_without_an_existing_mise_file {
    use super::*;

    mod when_synchronizing_workspace_mise {
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
            let mise_service = MiseService::new(config_service);

            // Act
            let managed_path = mise_service.sync_workspace_mise(&workspace_root).unwrap();
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

    mod when_synchronizing_workspace_mise {
        use super::*;

        #[test]
        fn should_preserve_user_defined_tools_and_tasks_while_adding_detected_language_defaults() {
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
            let managed_mise_path = workspace_root.join(".lgtm/mise.toml");
            std::fs::write(
                &managed_mise_path,
                "[tools]\nrust-analyzer = \"2026-01-01\"\n\n[tasks.check]\ndescription = \"Custom clippy check\"\nrun = \"cargo clippy --all-targets\"\n",
            )
            .unwrap();
            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::new(config_service);

            // Act
            let updated_path = mise_service.sync_workspace_mise(&workspace_root).unwrap();
            let updated = std::fs::read_to_string(&updated_path).unwrap();

            // Assert
            assert_eq!(updated_path, managed_mise_path);
            assert!(updated.contains("rust-analyzer = \"2026-01-01\""));
            assert!(updated.contains("cargo clippy --all-targets"));
            assert!(updated.contains("[tasks.\"rust:check\"]"));
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
                workspace_root.join(".lgtm/config.json"),
                "{\n  \"lsp\": {\n    \"enabled\": true,\n    \"auto_install\": true\n  }\n}\n",
            )
            .unwrap();
            std::fs::write(
                workspace_root.join(".lgtm/mise.toml"),
                "[tasks.check]\ndescription = \"Custom TS check\"\nrun = \"tsc -p tsconfig.json --noEmit\"\n",
            )
            .unwrap();

            let config_service = ConfigService::from_home(home);
            let mise_service = MiseService::new(config_service);

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

mod given_global_and_project_dev_tool_management {
    use super::*;

    mod when_adding_and_removing_dev_tools {
        use super::*;

        #[test]
        fn should_manage_global_and_workspace_dev_tools_in_mise_toml_separately_from_config_json() {
            // Arrange
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let workspace_root = temp.path().join("workspace");
            std::fs::create_dir_all(&workspace_root).unwrap();
            std::fs::write(
                workspace_root.join("Cargo.toml"),
                "[package]\nname = \"sample\"\n",
            )
            .unwrap();
            let config_service = ConfigService::from_home(home.clone());
            let mise_service = MiseService::new(config_service);

            // Act
            let global_added = mise_service.add_dev_tool(None, "biome", "1.9.4").unwrap();
            let project_added = mise_service
                .add_dev_tool(Some(&workspace_root), "cargo-nextest", "latest")
                .unwrap();
            let project_removed = mise_service
                .remove_dev_tool(Some(&workspace_root), "cargo-nextest")
                .unwrap();
            let global_removed = mise_service.remove_dev_tool(None, "biome").unwrap();

            // Assert
            assert_eq!(global_added.tools.get("biome"), Some(&"1.9.4".to_string()));
            assert!(home.join(".lgtm/mise.toml").is_file());
            assert_eq!(project_added.tools.get("biome"), Some(&"1.9.4".to_string()));
            assert_eq!(
                project_added.tools.get("cargo-nextest"),
                Some(&"latest".to_string())
            );
            assert_eq!(
                project_added.tools.get("rust-analyzer"),
                Some(&"latest".to_string())
            );
            assert!(!project_removed.tools.contains_key("cargo-nextest"));
            assert!(!global_removed.tools.contains_key("biome"));
        }
    }
}
