//! Real-process fixtures, isolated from the user's data and credentials.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------
pub struct Fixture {
    pub _home: TempDir,
    pub files: TempDir,
    pub child: Child,
    pub endpoint_url: String,
}

impl Fixture {
    pub fn rpc(&self, method: &str, input: Value, actor: Option<&str>) -> Value {
        let service = if method == "EnsureIdentity" || method == "GetCapabilities" {
            "IdentityService"
        } else {
            "ReviewService"
        };
        let mut request = reqwest::blocking::Client::new()
            .post(format!("{}/lgtm.v1.{service}/{method}", self.endpoint_url))
            .header("Connect-Protocol-Version", "1")
            .json(&input);
        if let Some(actor) = actor {
            request = request.header("x-lgtm-identity", actor);
        }
        let response = request.send().unwrap();
        let status = response.status();
        let body: Value = response.json().unwrap();
        assert!(status.is_success(), "{method}: {status}: {body}");
        body
    }

    pub fn start() -> Self {
        let home = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let child = Command::new(binary("lgtm-backend"))
            .env("LGTM_HOME", home.path())
            .env("HOME", home.path())
            .env("LGTM_BACKEND_ADDR", address.to_string())
            .env_remove("LGTM_DATABASE_URL")
            .env_remove("LGTM_RPC_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut fixture = Self {
            _home: home,
            files,
            child,
            endpoint_url: format!("http://{address}"),
        };
        let start = Instant::now();
        loop {
            let response = reqwest::blocking::Client::new()
                .post(format!(
                    "{}/lgtm.v1.IdentityService/GetCapabilities",
                    fixture.endpoint_url
                ))
                .json(&serde_json::json!({}))
                .send();
            if response.is_ok_and(|r| r.status().is_success()) {
                break;
            }
            assert!(
                fixture.child.try_wait().unwrap().is_none(),
                "backend exited during startup"
            );
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "backend startup timed out"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
        fixture
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn binary(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    target.join("debug").join(name)
}
