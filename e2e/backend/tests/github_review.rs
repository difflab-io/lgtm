//! Complete remote workflows through real processes and the local GitHub HTTP fixture.
#[allow(dead_code)]
mod common;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

// fixtures --------------------------------------------------------------------
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Remote {
    _process: Process,
    _temp: tempfile::TempDir,
    endpoint_url: String,
}
impl Remote {
    fn start() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let port_file = temp.path().join("endpoint_url");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mut process = Process(
            Command::new("python3")
                .arg(root.join("e2e/common/github_server.py"))
                .arg("--port-file")
                .arg(&port_file)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        let endpoint_url = loop {
            if let Ok(value) = std::fs::read_to_string(&port_file)
                && !value.is_empty()
            {
                break value;
            }
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "GitHub fixture exited"
            );
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(20));
        };
        Self {
            _process: process,
            _temp: temp,
            endpoint_url,
        }
    }
    fn mode(&self, mode: &str) {
        reqwest::blocking::Client::new()
            .post(format!("{}/__control", self.endpoint_url))
            .json(&json!({"mode":mode}))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
    }
    fn status(&self) -> Value {
        reqwest::blocking::get(format!("{}/__status", self.endpoint_url))
            .unwrap()
            .json()
            .unwrap()
    }
}
struct Backend {
    process: Process,
    home: tempfile::TempDir,
    endpoint_url: String,
    remote: Remote,
}
impl Backend {
    fn start() -> Self {
        let remote = Remote::start();
        let home = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let process = Process(
            Command::new(common::binary("lgtm-backend"))
                .env("LGTM_HOME", home.path())
                .env("HOME", home.path())
                .env("LGTM_BACKEND_ADDR", address.to_string())
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_DATABASE_URL")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut value = Self {
            process,
            home,
            endpoint_url: format!("http://{address}"),
            remote,
        };
        let start = Instant::now();
        loop {
            if reqwest::blocking::Client::new()
                .post(format!(
                    "{}/lgtm.v1.IdentityService/GetCapabilities",
                    value.endpoint_url
                ))
                .json(&json!({}))
                .send()
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            assert!(
                value.process.0.try_wait().unwrap().is_none(),
                "backend exited"
            );
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(20));
        }
        value
    }
    fn restart(&mut self) {
        let _ = self.process.0.kill();
        let _ = self.process.0.wait();
        self.process = Process(
            Command::new(common::binary("lgtm-backend"))
                .env("LGTM_HOME", self.home.path())
                .env("HOME", self.home.path())
                .env(
                    "LGTM_BACKEND_ADDR",
                    self.endpoint_url.trim_start_matches("http://"),
                )
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &self.remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_DATABASE_URL")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        loop {
            if reqwest::blocking::Client::new()
                .post(format!(
                    "{}/lgtm.v1.IdentityService/GetCapabilities",
                    self.endpoint_url
                ))
                .json(&json!({}))
                .send()
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            assert!(self.process.0.try_wait().unwrap().is_none());
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn response(
        &self,
        method: &str,
        input: Value,
        actor: Option<&str>,
    ) -> (reqwest::StatusCode, Value) {
        let service = if matches!(method, "EnsureIdentity" | "GetCapabilities") {
            "IdentityService"
        } else {
            "ReviewService"
        };
        let mut request = reqwest::blocking::Client::new()
            .post(format!("{}/lgtm.v1.{service}/{method}", self.endpoint_url))
            .json(&input);
        if let Some(actor) = actor {
            request = request.header("x-lgtm-identity", actor);
        }
        let response = request.send().unwrap();
        (response.status(), response.json().unwrap())
    }
    fn rpc(&self, method: &str, input: Value, actor: Option<&str>) -> Value {
        let (status, body) = self.response(method, input, actor);
        assert!(status.is_success(), "{method} {status}: {body}");
        body
    }
}

// tests -----------------------------------------------------------------------
#[test]
fn should_select_remote_content_submit_and_reconcile_without_checkout_or_duplicate_writes() {
    for mode in ["normal", "lost", "stale"] {
        let mut fixture = Backend::start();
        let actor = fixture.rpc(
            "EnsureIdentity",
            json!({"name":"Remote reviewer","email":"reviewer@example.invalid"}),
            None,
        )["actor"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let pulls = fixture.rpc(
            "ListPullRequests",
            json!({"repositoryUrl":"https://github.com/acme/repo","query":"E2E"}),
            None,
        );
        assert_eq!(pulls["pullRequests"].as_array().unwrap().len(), 3);
        let context = fixture.rpc("EnsureContext",json!({"key":{"kind":"forge_backed","repositoryUrl":"https://github.com/acme/repo","number":"1"}}),None)["context"]["id"].as_str().unwrap().to_owned();
        fixture.rpc("RefreshContext", json!({"contextId":context}), None);
        let workspace = fixture.rpc("GetWorkspace", json!({"contextId":context}), None);
        assert!(
            workspace["files"][0]["baseContent"]
                .as_str()
                .unwrap()
                .contains("old")
        );
        assert!(
            workspace["files"][0]["content"]
                .as_str()
                .unwrap()
                .contains("new")
        );
        assert!(!fixture.home.path().join(".git").exists());
        let session = fixture.rpc(
            "StartSession",
            json!({"contextId":context,"base":"base","head":"head"}),
            Some(&actor),
        )["session"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        for (body, anchor) in [
            ("General remote concern", json!({"kind":"general"})),
            (
                "Inline remote concern",
                json!({"kind":"line","path":"review.txt","start":"2","end":"2","side":"new","commit":"head"}),
            ),
            (
                "File remote concern",
                json!({"kind":"file","path":"review.txt"}),
            ),
        ] {
            fixture.rpc(
                "AddComment",
                json!({"sessionId":session,"body":body,"anchor":anchor}),
                Some(&actor),
            );
        }
        fixture.remote.mode(mode);
        let input = json!({"sessionId":session,"outcome":"approve","body":"Final E2E body","requestKey":"remote-once","expectedHead":"head"});
        if mode == "stale" {
            let (status, _) = fixture.response("SubmitReview", input, Some(&actor));
            assert!(!status.is_success());
            assert_eq!(fixture.remote.status()["writes"], 0);
            assert_eq!(
                fixture.rpc("GetSession", json!({"sessionId":session}), None)["session"]["state"],
                "active"
            );
            assert_eq!(
                fixture.rpc(
                    "GetConversation",
                    json!({"contextId":context,"includeDrafts":true}),
                    None
                )["comments"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            continue;
        }
        let submitted = fixture.rpc("SubmitReview", input.clone(), Some(&actor));
        assert_eq!(
            submitted["session"]["submissionStatus"],
            if mode == "lost" {
                "uncertain"
            } else {
                "confirmed"
            }
        );
        let before = fixture.remote.status();
        assert_eq!(before["reviews"][0]["state"], "APPROVED");
        assert!(
            before["reviews"][0]["body"]
                .as_str()
                .unwrap()
                .contains("Final E2E body")
        );
        fixture.remote.mode("normal");
        if mode == "lost" {
            fixture.restart();
        }
        let recovered = fixture.rpc(
            "ReconcileSubmission",
            json!({"sessionId":session}),
            Some(&actor),
        );
        assert_eq!(recovered["session"]["submissionStatus"], "confirmed");
        fixture.rpc("SubmitReview", input, Some(&actor));
        assert_eq!(fixture.remote.status()["writes"], before["writes"]);
        assert_eq!(fixture.remote.status()["errors"], json!([]));
        let export = fixture.rpc("Export", json!({"sessionId":session}), None);
        assert!(
            export["markdown"]
                .as_str()
                .unwrap()
                .contains("Final E2E body")
        );
        assert!(
            export["markdown"]
                .as_str()
                .unwrap()
                .contains("Inline remote concern")
        );
    }
}

#[test]
fn should_preserve_drafts_when_github_permission_is_denied() {
    let fixture = Backend::start();
    let actor = fixture.rpc(
        "EnsureIdentity",
        json!({"name":"Permission reviewer","email":"permission@example.invalid"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let context = fixture.rpc("EnsureContext",json!({"key":{"kind":"forge_backed","repositoryUrl":"https://github.com/acme/repo","number":"1"}}),None)["context"]["id"].as_str().unwrap().to_owned();
    fixture.rpc("RefreshContext", json!({"contextId":context}), None);
    let session = fixture.rpc(
        "StartSession",
        json!({"contextId":context,"base":"base","head":"head"}),
        Some(&actor),
    )["session"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    fixture.rpc(
        "AddComment",
        json!({"sessionId":session,"body":"Retain after denial"}),
        Some(&actor),
    );
    fixture.remote.mode("denied");
    let (status, error) = fixture.response("RefreshContext", json!({"contextId":context}), None);
    assert!(!status.is_success());
    assert!(error["message"].is_string());
    let conversation = fixture.rpc(
        "GetConversation",
        json!({"contextId":context,"includeDrafts":true}),
        None,
    );
    assert_eq!(conversation["comments"][0]["body"], "Retain after denial");
    assert_eq!(fixture.remote.status()["writes"], 0);
    fixture.remote.mode("normal");
    fixture.rpc("RefreshContext", json!({"contextId":context}), None);
    assert_eq!(fixture.remote.status()["errors"], json!([]));
}
