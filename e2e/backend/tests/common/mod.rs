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
    pub fn response(
        &self,
        method: &str,
        input: Value,
        actor: Option<&str>,
    ) -> (reqwest::StatusCode, Value) {
        let service = if method == "EnsureIdentity" || method == "GetCapabilities" {
            "IdentityService"
        } else if ["GetDiff", "GetFiles", "GetFile"].contains(&method) {
            "DiffService"
        } else {
            "ReviewService"
        };
        let mut request = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap()
            .post(format!("{}/lgtm.v1.{service}/{method}", self.endpoint_url))
            .header("Connect-Protocol-Version", "1")
            .json(&input);
        if let Some(actor) = actor {
            request = request.header("x-lgtm-identity", actor);
        }
        let response = request.send().unwrap();
        let status = response.status();
        let body: Value = response.json().unwrap();
        (status, body)
    }

    pub fn rpc(&self, method: &str, input: Value, actor: Option<&str>) -> Value {
        let (status, body) = self.response(method, input, actor);
        assert!(status.is_success(), "{method}: {status}: {body}");
        body
    }

    pub fn start() -> Self {
        let artifacts = binary("lgtm-backend")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("e2e/backend");
        std::fs::create_dir_all(&artifacts).unwrap();
        let home = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(artifacts)
            .unwrap();
        let files = tempfile::Builder::new()
            .prefix("workspace-")
            .tempdir_in(home.path())
            .unwrap();
        let log = std::fs::File::create(home.path().join("backend.log")).unwrap();
        let child = Command::new(binary("lgtm-backend"))
            .env("LGTM_HOME", home.path())
            .env("HOME", home.path())
            .env("GIT_CEILING_DIRECTORIES", home.path())
            .env("LGTM_BACKEND_ADDR", "127.0.0.1:0")
            .env_remove("LGTM_DATABASE_URL")
            .env_remove("LGTM_RPC_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let mut fixture = Self {
            _home: home,
            files,
            child,
            endpoint_url: String::new(),
        };
        let start = Instant::now();
        loop {
            if let Some(endpoint) = get_endpoint(fixture._home.path()) {
                fixture.endpoint_url = endpoint;
            }
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
        if std::thread::panicking() {
            self._home.disable_cleanup(true);
            self.files.disable_cleanup(true);
            eprintln!(
                "Failed backend fixture retained at {}",
                self._home.path().display()
            );
            if let Ok(log) = std::fs::read_to_string(self._home.path().join("backend.log")) {
                eprintln!("{log}");
            }
        }
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

/// Read the address printed by the backend after it owns its listening socket.
pub fn get_endpoint(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join("backend.log"))
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("LGTM backend listening on ")
                .map(str::to_owned)
        })
}
