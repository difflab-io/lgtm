pub mod proto {
    connectrpc::include_generated!();
}

use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lgtm"))
        .arg("--state-dir")
        .arg(dir)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

struct Instance(TempDir);
impl Drop for Instance {
    fn drop(&mut self) {
        command(self.0.path(), &["down"]);
    }
}

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
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("instance.json")).unwrap()).unwrap();
    let url = format!("http://127.0.0.1:{}", state["port"]);
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
    assert_eq!(
        client
            .post(format!("{url}/_lgtm/shutdown"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let again = command(dir, &["up", "--port", "0", "--no-open"]);
    assert!(again.status.success());
    let reused: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("instance.json")).unwrap()).unwrap();
    assert_eq!(state["pid"], reused["pid"]);
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
        .arg("--state-dir")
        .arg(dir)
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
    assert!(!dir.join("instance.json").exists());
    let status = command(dir, &["status"]);
    assert!(String::from_utf8_lossy(&status.stdout).contains("not running"));
    let status = command(dir, &["status", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
        serde_json::json!({"running": false})
    );
}

#[test]
fn occupied_port_fails_without_leaving_an_instance() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let dir = tempfile::tempdir().unwrap();
    let result = command(dir.path(), &["up", "--port", &port, "--no-open"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("did not become ready"));
    assert!(!dir.path().join("instance.json").exists());
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
