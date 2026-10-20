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
        let remote = Self {
            _process: process,
            _temp: temp,
            endpoint_url,
        };
        remote.mode("normal");
        remote
    }
    fn mode(&self, mode: &str) {
        reqwest::blocking::Client::new()
            .post(format!("{}/__control", self.endpoint_url))
            .json(&json!({"mode":mode,"base":"a".repeat(40),"head":"b".repeat(40)}))
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
struct Cli {
    process: Process,
    home: tempfile::TempDir,
    endpoint_url: String,
    remote: Remote,
}
impl Cli {
    fn start() -> Self {
        let remote = Remote::start();
        let home = tempfile::tempdir().unwrap();
        let process = Process(
            Command::new(common::binary("lgtm"))
                .args(["up", "--foreground", "--no-open", "--port", "0"])
                .env("LGTM_HOME", home.path())
                .env("HOME", home.path())
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_RPC_URL")
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
            endpoint_url: String::new(),
            remote,
        };
        let start = Instant::now();
        loop {
            if let Ok(port) = std::fs::read_to_string(value.home.path().join("server.port")) {
                value.endpoint_url = format!("http://127.0.0.1:{}/rpc", port.trim());
                break;
            }
            assert!(
                value.process.0.try_wait().unwrap().is_none(),
                "daemon exited"
            );
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(20));
        }
        value
    }
    fn restart(&mut self) {
        let _ = self.process.0.kill();
        let _ = self.process.0.wait();
        let _ = std::fs::remove_file(self.home.path().join("server.port"));
        self.process = Process(
            Command::new(common::binary("lgtm"))
                .args(["up", "--foreground", "--no-open", "--port", "0"])
                .env("LGTM_HOME", self.home.path())
                .env("HOME", self.home.path())
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &self.remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_RPC_URL")
                .env_remove("LGTM_DATABASE_URL")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        loop {
            if let Ok(port) = std::fs::read_to_string(self.home.path().join("server.port")) {
                self.endpoint_url = format!("http://127.0.0.1:{}/rpc", port.trim());
                break;
            }
            assert!(self.process.0.try_wait().unwrap().is_none());
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn command(&self, args: &[&str]) -> std::process::Output {
        Command::new(common::binary("lgtm"))
            .args(args)
            .arg("--json")
            .env("LGTM_HOME", self.home.path())
            .env("HOME", self.home.path())
            .env("LGTM_RPC_URL", &self.endpoint_url)
            .env("LGTM_NAME", "CLI remote reviewer")
            .env("LGTM_EMAIL", "cli-e2e@example.invalid")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .current_dir(self.home.path())
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let result = self.command(args);
        assert!(
            result.status.success(),
            "args={args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
}

// tests -----------------------------------------------------------------------
#[test]
fn should_run_remote_cli_review_and_recover_an_uncertain_submission_without_replay() {
    let pr = "https://github.com/acme/repo/pull/1";
    for mode in ["normal", "lost", "stale"] {
        let mut fixture = Cli::start();
        let started = fixture.json(&["session", "start", "--pr", pr]);
        assert_eq!(
            started["review"]["state"], "active",
            "Starting a PR needs no setup or explicit refresh"
        );
        let refresh = fixture.json(&["session", "refresh", "--pr", pr]);
        assert_eq!(
            refresh["review"]["observedComparison"]["pullRequest"]["headCommit"],
            "b".repeat(40)
        );
        let resumed = fixture.json(&["session", "start", "--pr", pr]);
        assert_eq!(
            resumed["review"]["id"], started["review"]["id"],
            "matching start resumes the same attempt"
        );
        fixture.json(&[
            "comments",
            "add",
            "--pr",
            pr,
            "--body",
            "General CLI comment",
        ]);
        fixture.json(&[
            "comments",
            "add",
            "--pr",
            pr,
            "--file",
            "review.txt",
            "--line",
            "2",
            "--body",
            "Inline CLI comment",
        ]);
        fixture.json(&[
            "comments",
            "add",
            "--pr",
            pr,
            "--file",
            "review.txt",
            "--body",
            "File CLI comment",
        ]);
        let pending = fixture.remote.status();
        assert_eq!(
            pending["reviews"][0]["state"], "PENDING",
            "inline/file saves create a pending review before submission"
        );
        let writes_before_submit = pending["writes"].clone();
        fixture.remote.mode(mode);
        let submit = [
            "session",
            "submit",
            "--pr",
            pr,
            "--outcome",
            "approve",
            "--body",
            "Final CLI body",
            "--request-key",
            "cli-once",
        ];
        if mode == "stale" {
            let output = fixture.command(&submit);
            assert!(!output.status.success());
            let error: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(error["error"]["message"].as_str().unwrap().contains("head"));
            assert_eq!(fixture.remote.status()["writes"], writes_before_submit);
            let conversation = fixture.json(&["comments", "list", "--pr", pr]);
            assert_eq!(conversation["comments"].as_array().unwrap().len(), 3);
            assert_eq!(
                fixture.json(&["session", "show", "--pr", pr])["review"]["state"],
                "active"
            );
            continue;
        }
        let submitted = fixture.json(&submit);
        assert_eq!(
            submitted["review"]["submissionStatus"],
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
                .contains("Final CLI body")
        );
        fixture.remote.mode("normal");
        if mode == "lost" {
            fixture.restart();
        }
        let recovered = if mode == "lost" {
            fixture.json(&["session", "reconcile", "--pr", pr])
        } else {
            fixture.json(&submit)
        };
        assert_eq!(recovered["review"]["submissionStatus"], "confirmed");
        assert_eq!(fixture.remote.status()["writes"], before["writes"]);
        let export = fixture.json(&["session", "export", "--pr", pr]);
        assert!(
            export["markdown"]
                .as_str()
                .unwrap()
                .contains("Inline CLI comment")
        );
        assert!(
            export["markdown"]
                .as_str()
                .unwrap()
                .contains("Final CLI body")
        );
        assert_eq!(fixture.remote.status()["errors"], json!([]));
    }
}

#[test]
fn should_report_json_permission_error_and_keep_remote_review_drafts() {
    let fixture = Cli::start();
    let pr = "https://github.com/acme/repo/pull/1";
    fixture.json(&["session", "start", "--pr", pr]);
    fixture.json(&["session", "refresh", "--pr", pr]);
    fixture.json(&[
        "comments",
        "add",
        "--pr",
        pr,
        "--body",
        "Retain denied CLI draft",
    ]);
    fixture.remote.mode("denied");
    let output = fixture.command(&["session", "refresh", "--pr", pr]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(error["error"]["message"].is_string());
    let conversation = fixture.json(&["comments", "list", "--pr", pr]);
    assert_eq!(
        conversation["comments"][0]["body"],
        "Retain denied CLI draft"
    );
    assert_eq!(fixture.remote.status()["writes"], 0);
    fixture.remote.mode("normal");
    fixture.json(&["session", "refresh", "--pr", pr]);
    assert_eq!(fixture.remote.status()["errors"], json!([]));
}

#[test]
fn should_recover_a_saved_pending_draft_without_replaying_the_accepted_comment() {
    let fixture = Cli::start();
    let pr = "https://github.com/acme/repo/pull/1";
    fixture.json(&["session", "start", "--pr", pr]);
    fixture.json(&["session", "refresh", "--pr", pr]);
    fixture.remote.mode("draft_lost");
    let saved = fixture.json(&[
        "comments",
        "add",
        "--pr",
        pr,
        "--file",
        "review.txt",
        "--line",
        "2",
        "--body",
        "Recover this private draft",
    ]);
    assert!(
        !saved["syncError"].as_str().unwrap().is_empty(),
        "local save reports failed GitHub confirmation"
    );
    assert_eq!(saved["comment"]["postingStatus"], "uncertain");
    let before = fixture.remote.status()["writes"].clone();
    fixture.remote.mode("normal");
    let id = saved["comment"]["id"].as_str().unwrap();
    let recovered = fixture.json(&["comments", "sync", id, "--pr", pr]);
    assert_eq!(recovered["comment"]["postingStatus"], "confirmed");
    assert_eq!(recovered["comment"]["visibility"], "draft");
    assert_eq!(fixture.remote.status()["writes"], before);
    assert_eq!(fixture.remote.status()["reviews"][0]["state"], "PENDING");
}
