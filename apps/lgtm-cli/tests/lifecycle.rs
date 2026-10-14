pub mod proto {
    connectrpc::include_generated!();
}

use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

// types -----------------------------------------------------------------------

struct Instance(TempDir);
impl Drop for Instance {
    fn drop(&mut self) {
        command(self.0.path(), &["down"]);
    }
}

// api -------------------------------------------------------------------------

#[tokio::test]
async fn background_instance_serves_ui_and_rpc_and_stops() {
    let instance = Instance(tempfile::tempdir().unwrap());
    let dir = instance.0.path();
    let up = command(dir, &["up", "--port", "0", "--no-open"]);
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stderr)
    );
    let state = discovery(dir);
    let url = state["ui_url"].as_str().unwrap().to_owned();
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
    assert_eq!(reply.into_owned().message, "Hello, gRPC!");

    let page = client.get(&url).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("LGTM Hello World"));
    for prefix in ["", "/rpc"] {
        let response = client
            .post(format!("{url}{prefix}/lgtm.v1.GreeterService/SayHello"))
            .json(&serde_json::json!({"name":"Package"}))
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
    assert_eq!(
        client
            .post(format!("{url}/_lgtm/shutdown"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let again = command(dir, &["up", "--port", "0", "--no-open"]);
    assert!(again.status.success());
    let reused = discovery(dir);
    assert_eq!(state["pid"], reused["pid"]);
    let foreground = command(dir, &["up", "--foreground", "--port", "0"]);
    assert!(!foreground.status.success());
    assert!(String::from_utf8_lossy(&foreground.stderr).contains("Another LGTM instance"));
    let status = command(dir, &["status"]);
    assert!(status.status.success());
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(text.contains(&format!("UI: {url}")));
    assert!(text.contains(&format!("Server: {url}")));
    assert!(text.contains(&format!("Browser RPC: {url}/rpc")));
    let status = command(dir, &["status", "--json"]);
    let discovery: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(discovery["running"], true);
    assert_eq!(discovery["pid"], state["pid"]);
    assert_eq!(discovery["ui_url"], url);
    assert_eq!(discovery["server_url"], url);
    assert_eq!(discovery["rpc_url"], format!("{url}/rpc"));
    assert!(discovery.get("token").is_none());

    let cwd = dir
        .canonicalize()
        .unwrap()
        .join("review space & plus+ café #file");
    std::fs::create_dir(&cwd).unwrap();
    let review = Command::new(env!("CARGO_BIN_EXE_lgtm"))
        .env("LGTM_HOME", dir)
        .args(["review", "--no-open"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(review.status.success());
    let review_url = reqwest::Url::parse(String::from_utf8_lossy(&review.stdout).trim()).unwrap();
    assert_eq!(review_url.origin().ascii_serialization(), url);
    let query: Vec<_> = review_url.query_pairs().collect();
    assert_eq!(query, [("path".into(), cwd.to_str().unwrap().into())]);
    assert_eq!(client.get(review_url).send().await.unwrap().status(), 200);
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
fn occupied_port_fails_without_leaving_an_instance() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let dir = tempfile::tempdir().unwrap();
    let result = command(dir.path(), &["up", "--port", &port, "--no-open"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("did not become ready"));
    assert!(!dir.path().join("server.port").exists());
    assert!(
        std::fs::read_to_string(dir.path().join("server.log"))
            .unwrap()
            .contains("Address already in use")
    );
}

#[test]
fn review_starts_a_ready_server_for_the_current_directory() {
    let instance = Instance(tempfile::tempdir().unwrap());
    let dir = instance.0.path();
    let review = command(dir, &["review", "--port", "0", "--no-open"]);
    assert!(
        review.status.success(),
        "{}",
        String::from_utf8_lossy(&review.stderr)
    );
    let url = reqwest::Url::parse(String::from_utf8_lossy(&review.stdout).trim()).unwrap();
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        [(
            "path".into(),
            dir.canonicalize().unwrap().to_str().unwrap().into()
        )]
    );
    let status = command(dir, &["status", "--json"]);
    let discovery: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(discovery["running"], true);
    assert_eq!(discovery["ui_url"], url.origin().ascii_serialization());
}

#[test]
fn simultaneous_up_calls_share_one_daemon() {
    let instance = Instance(tempfile::tempdir().unwrap());
    let dir = instance.0.path();
    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_lgtm"))
            .env("LGTM_HOME", dir)
            .args(["up", "--port", "0", "--no-open"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = spawn();
    let second = spawn();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
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
fn stale_pid_does_not_stop_another_server() {
    let first = Instance(tempfile::tempdir().unwrap());
    let second = Instance(tempfile::tempdir().unwrap());
    for instance in [&first, &second] {
        assert!(
            command(instance.0.path(), &["up", "--port", "0", "--no-open"])
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
    let down = command(stale.path(), &["down"]);
    assert!(down.status.success());
    assert!(String::from_utf8_lossy(&down.stdout).contains("not running"));
    for instance in [&first, &second] {
        assert_eq!(discovery(instance.0.path())["running"], true);
    }
}

#[test]
fn status_on_unused_home_does_not_create_files() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("unused home");
    let status = Command::new(env!("CARGO_BIN_EXE_lgtm"))
        .env("LGTM_HOME", &home)
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
        serde_json::json!({"running": false})
    );
    assert!(!home.exists());
}

// helpers ---------------------------------------------------------------------

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
