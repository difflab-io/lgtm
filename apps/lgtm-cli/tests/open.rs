//! Integration tests for `lgtm open [path]` and positional `lgtm [path]` desktop launcher.

use std::process::Command;

// tests -----------------------------------------------------------------------

mod when_running_open {
    use super::*;

    mod given_desktop_binary_is_missing {
        use super::*;

        #[test]
        fn should_return_not_found_error_in_json() {
            // Arrange
            let root = tempfile::tempdir().unwrap();
            let workspace = root.path().join("my-repo");
            std::fs::create_dir_all(&workspace).unwrap();
            let missing_bin = root.path().join("missing-lgtm-desktop");

            // Act
            let open_out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", root.path())
                .env("LGTM_DESKTOP_BIN", &missing_bin)
                .current_dir(root.path())
                .args(["open", "my-repo", "--json"])
                .output()
                .unwrap();

            // Assert
            assert!(!open_out.status.success());
            let payload: serde_json::Value = serde_json::from_slice(&open_out.stdout).unwrap();
            assert_eq!(payload["error"]["code"], "not_found");
        }
    }

    mod given_desktop_binary_is_present_and_workspace_is_missing {
        use super::*;

        #[test]
        fn should_return_invalid_argument_error_in_json() {
            // Arrange
            let root = tempfile::tempdir().unwrap();

            // Act
            let open_out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", root.path())
                .current_dir(root.path())
                .args(["open", "does-not-exist", "--json"])
                .output()
                .unwrap();

            // Assert
            assert!(!open_out.status.success());
            let payload: serde_json::Value = serde_json::from_slice(&open_out.stdout).unwrap();
            assert_eq!(payload["error"]["code"], "invalid_argument");
        }
    }

    mod given_desktop_binary_and_workspace_are_present {
        use super::*;

        #[cfg(unix)]
        #[test]
        fn should_resolve_relative_directory_and_invoke_configured_desktop_binary() {
            use std::os::unix::fs::PermissionsExt;

            // Arrange
            let root = tempfile::tempdir().unwrap();
            let workspace = root.path().join("my-repo");
            std::fs::create_dir_all(&workspace).unwrap();
            let marker = root.path().join("launched.txt");
            let fake_desktop = root.path().join("fake-lgtm-desktop");
            std::fs::write(
                &fake_desktop,
                format!(
                    "#!/bin/sh\nprintf '%s\\n%s\\n' \"$1\" \"$LGTM_WORKSPACE\" > \"{}\"\n",
                    marker.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&fake_desktop, std::fs::Permissions::from_mode(0o755))
                .unwrap();

            // Act
            let open_out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", root.path())
                .env("LGTM_DESKTOP_BIN", &fake_desktop)
                .current_dir(root.path())
                .args(["open", "my-repo", "--json"])
                .output()
                .unwrap();

            // Assert
            assert!(
                open_out.status.success(),
                "{}",
                String::from_utf8_lossy(&open_out.stderr)
            );
            let payload: serde_json::Value = serde_json::from_slice(&open_out.stdout).unwrap();
            let canonical_ws = std::fs::canonicalize(&workspace)
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(payload["workspace_path"], canonical_ws);

            for _ in 0..50 {
                if marker.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let launched = std::fs::read_to_string(&marker).unwrap();
            let lines: Vec<&str> = launched.lines().collect();
            assert_eq!(lines, vec![canonical_ws.as_str(), canonical_ws.as_str()]);
        }

        #[cfg(windows)]
        #[test]
        fn should_resolve_relative_directory_and_invoke_configured_desktop_binary() {
            // Arrange
            let root = tempfile::tempdir().unwrap();
            let workspace = root.path().join("my-repo");
            std::fs::create_dir_all(&workspace).unwrap();
            let marker = root.path().join("launched.txt");
            let fake_desktop = root.path().join("fake-lgtm-desktop.cmd");
            std::fs::write(
                &fake_desktop,
                format!(
                    "@echo off\r\n(\r\necho %~1\r\necho %LGTM_WORKSPACE%\r\n) > \"{}\"\r\n",
                    marker.display()
                ),
            )
            .unwrap();

            // Act
            let open_out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", root.path())
                .env("LGTM_DESKTOP_BIN", &fake_desktop)
                .current_dir(root.path())
                .args(["open", "my-repo", "--json"])
                .output()
                .unwrap();

            // Assert
            assert!(
                open_out.status.success(),
                "{}",
                String::from_utf8_lossy(&open_out.stderr)
            );
            let payload: serde_json::Value = serde_json::from_slice(&open_out.stdout).unwrap();
            let canonical_ws = std::fs::canonicalize(&workspace)
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(payload["workspace_path"], canonical_ws);

            for _ in 0..50 {
                if marker.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let launched = std::fs::read_to_string(&marker).unwrap();
            let lines: Vec<&str> = launched.lines().map(str::trim).collect();
            assert_eq!(lines, vec![canonical_ws.as_str(), canonical_ws.as_str()]);
        }
    }
}
