//! Integration tests for the `lgtm-cli` background RPC daemon (`up`, `status`, `down`) and `open` desktop launcher.

use lgtm_forge::proto;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

struct Instance(TempDir);

impl Drop for Instance {
    fn drop(&mut self) {
        command(self.0.path(), &["down"]);
    }
}

fn command(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lgtm"))
        .env("LGTM_HOME", dir)
        .env("RUST_LOG", "info")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn discovery(dir: &Path) -> serde_json::Value {
    let status = command(dir, &["status", "--json"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    serde_json::from_slice(&status.stdout).unwrap()
}

// tests -----------------------------------------------------------------------

mod given_a_stopped_daemon {
    use super::*;

    mod when_running_up_status_and_down {
        use super::*;

        #[tokio::test]
        async fn should_serve_rpc_and_health_from_background_instance_and_stop() {
            // Arrange
            let instance = Instance(tempfile::tempdir().unwrap());
            let dir = instance.0.path();

            // Act
            let up = command(dir, &["up", "--port", "0"]);
            assert!(
                up.status.success(),
                "{}",
                String::from_utf8_lossy(&up.stderr)
            );
            let state = discovery(dir);
            let url = state["server_url"].as_str().unwrap().to_owned();
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let grpc = proto::lgtm::v1::GreeterServiceClient::new(
                connectrpc::client::HttpClient::plaintext_http2_only(),
                connectrpc::client::ClientConfig::new(url.parse().unwrap())
                    .with_protocol(connectrpc::Protocol::Grpc),
            );
            let reply = grpc
                .say_hello(proto::lgtm::v1::SayHelloRequest {
                    name: "gRPC".into(),
                    ..Default::default()
                })
                .await
                .unwrap();

            // Assert
            assert_eq!(reply.into_owned().message, "Hello, gRPC!");
            assert!(state.get("ui_url").is_none());

            let root_get = client.get(&url).send().await.unwrap();
            assert_eq!(root_get.status(), 404);

            for prefix in ["", "/rpc"] {
                let response = client
                    .post(format!("{url}{prefix}/lgtm.v1.GreeterService/SayHello"))
                    .json(&serde_json::json!({"name": "Package"}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                assert_eq!(
                    response.json::<serde_json::Value>().await.unwrap()["message"],
                    "Hello, Package!"
                );
            }
            assert_eq!(
                client
                    .get(format!("{url}/missing.js"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                404
            );
            let health = client.get(format!("{url}/health")).send().await.unwrap();
            assert_eq!(health.status(), 200);
            assert_eq!(
                health.json::<serde_json::Value>().await.unwrap()["pid"],
                state["pid"]
            );

            let again = command(dir, &["up", "--port", "0", "--json"]);
            assert!(again.status.success());
            let again_json: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
            assert_eq!(again_json["pid"], state["pid"]);
            assert_eq!(again_json["server_url"], url);
            assert_eq!(again_json["rpc_url"], format!("{url}/rpc"));
            assert!(again_json.get("ui_url").is_none());

            let foreground = command(dir, &["up", "--foreground", "--port", "0"]);
            assert!(!foreground.status.success());
            assert!(String::from_utf8_lossy(&foreground.stderr).contains("Another LGTM instance"));

            let status = command(dir, &["status"]);
            assert!(status.status.success());
            let text = String::from_utf8_lossy(&status.stdout);
            assert!(!text.contains("UI:"));
            assert!(text.contains(&format!("Server: {url}")));
            assert!(text.contains(&format!("RPC: {url}/rpc")));

            assert!(command(dir, &["down"]).status.success());
            assert!(!dir.join("server.port").exists());
            let status = command(dir, &["status"]);
            assert!(String::from_utf8_lossy(&status.stdout).contains("not running"));
            let status = command(dir, &["status", "--json"]);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
                serde_json::json!({"running": false})
            );
            let log = std::fs::read_to_string(dir.join("server.log")).unwrap();
            assert!(log.contains("INFO") && log.contains("Server started"));
            assert!(log.contains("Server stopped"));
            assert!(!log.contains('\u{1b}'));
        }

        #[test]
        fn should_share_one_daemon_between_simultaneous_up_calls() {
            // Arrange
            let instance = Instance(tempfile::tempdir().unwrap());
            let dir = instance.0.path();
            let spawn = || {
                Command::new(env!("CARGO_BIN_EXE_lgtm"))
                    .env("LGTM_HOME", dir)
                    .args(["up", "--port", "0"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap()
            };

            // Act
            let first = spawn();
            let second = spawn();
            let first = first.wait_with_output().unwrap();
            let second = second.wait_with_output().unwrap();

            // Assert
            for result in [&first, &second] {
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(
                    result.stderr.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            assert_eq!(first.stdout, second.stdout);
            let state = discovery(dir);
            let pid = std::fs::read_to_string(dir.join("server.pid")).unwrap();
            assert_eq!(
                pid.trim().parse::<u64>().unwrap(),
                state["pid"].as_u64().unwrap()
            );
            assert!(!dir.join("start.lock").exists());
        }

        #[test]
        fn should_preserve_other_server_when_pid_is_stale() {
            // Arrange
            let first = Instance(tempfile::tempdir().unwrap());
            let second = Instance(tempfile::tempdir().unwrap());
            for instance in [&first, &second] {
                assert!(
                    command(instance.0.path(), &["up", "--port", "0"])
                        .status
                        .success()
                );
            }
            let first_info = discovery(first.0.path());
            let second_info = discovery(second.0.path());
            let stale = tempfile::tempdir().unwrap();
            let url = reqwest::Url::parse(first_info["server_url"].as_str().unwrap()).unwrap();
            std::fs::write(
                stale.path().join("server.pid"),
                second_info["pid"].to_string(),
            )
            .unwrap();
            std::fs::write(
                stale.path().join("server.port"),
                url.port().unwrap().to_string(),
            )
            .unwrap();

            // Act
            let down = command(stale.path(), &["down"]);

            // Assert
            assert!(down.status.success());
            assert!(String::from_utf8_lossy(&down.stdout).contains("not running"));
            for instance in [&first, &second] {
                assert_eq!(discovery(instance.0.path())["running"], true);
            }
        }

        #[test]
        fn should_leave_unused_home_untouched_when_reading_status() {
            // Arrange
            let root = tempfile::tempdir().unwrap();
            let home = root.path().join("unused home");

            // Act
            let status = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", &home)
                .args(["status", "--json"])
                .output()
                .unwrap();

            // Assert
            assert!(status.status.success());
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
                serde_json::json!({"running": false})
            );
            assert!(!home.exists());
        }
    }
}

mod given_an_occupied_port {
    use super::*;

    mod when_running_up {
        use super::*;

        #[test]
        fn should_fail_without_leaving_stale_port_file() {
            // Arrange
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port().to_string();
            let dir = tempfile::tempdir().unwrap();

            // Act
            let result = command(dir.path(), &["up", "--port", &port]);

            // Assert
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("did not become ready"));
            assert!(!dir.path().join("server.port").exists());
            assert!(
                std::fs::read_to_string(dir.path().join("server.log"))
                    .unwrap()
                    .contains("Address already in use")
            );
        }
    }
}

mod given_a_workspace_directory {
    use super::*;

    mod when_running_open {
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
    }
}
