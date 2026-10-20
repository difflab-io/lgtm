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
}
struct Backend {
    process: Process,
    home: tempfile::TempDir,
    endpoint_url: String,
    remote: Remote,
}
impl Backend {
    fn restart(&mut self) {
        let _ = self.process.0.kill();
        let _ = self.process.0.wait();
        let log = std::fs::File::create(self.home.path().join("backend.log")).unwrap();
        self.process = Process(
            Command::new(common::binary("lgtm-backend"))
                .env("LGTM_HOME", self.home.path())
                .env("HOME", self.home.path())
                .env("LGTM_BACKEND_ADDR", "127.0.0.1:0")
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &self.remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_DATABASE_URL")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        loop {
            if let Some(endpoint) = common::get_endpoint(self.home.path()) {
                self.endpoint_url = endpoint;
            }
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
        } else if ["GetDiff", "GetFiles", "GetFile"].contains(&method) {
            "DiffService"
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

    fn start() -> Self {
        let remote = Remote::start();
        let home = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(home.path().join("backend.log")).unwrap();
        let process = Process(
            Command::new(common::binary("lgtm-backend"))
                .env("LGTM_HOME", home.path())
                .env("HOME", home.path())
                .env("LGTM_BACKEND_ADDR", "127.0.0.1:0")
                .env("LGTM_GITHUB_TEST_MODE", "1")
                .env("LGTM_GITHUB_TEST_API_URL", &remote.endpoint_url)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env_remove("LGTM_DATABASE_URL")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(log))
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
            if let Some(endpoint) = common::get_endpoint(value.home.path()) {
                value.endpoint_url = endpoint;
            }
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
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.process.0.kill();
        let _ = self.process.0.wait();
        if std::thread::panicking() {
            self.home.disable_cleanup(true);
            eprintln!(
                "Failed GitHub backend fixture retained at {}",
                self.home.path().display()
            );
            if let Ok(log) = std::fs::read_to_string(self.home.path().join("backend.log")) {
                eprintln!("{log}");
            }
        }
    }
}

// tests -----------------------------------------------------------------------
#[test]
fn should_select_remote_content_submit_and_reconcile_without_checkout_or_duplicate_writes() {
    for mode in ["normal", "lost", "stale"] {
        let mut fixture = Backend::start();
        let base = "a".repeat(40);
        let head = "b".repeat(40);
        reqwest::blocking::Client::new()
            .post(format!("{}/__control", fixture.remote.endpoint_url))
            .json(&json!({"base":base,"head":head}))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        let actor = fixture.rpc(
            "EnsureIdentity",
            json!({"name":"Remote reviewer","email":"reviewer@example.invalid"}),
            None,
        )["actor"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let target =
            json!({"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}});
        let pulls = fixture.rpc(
            "ListPullRequests",
            json!({"repositoryUrl":"https://github.com/acme/repo","query":"E2E"}),
            None,
        );
        assert_eq!(pulls["pullRequests"].as_array().unwrap().len(), 3);
        let review = fixture.rpc(
            "StartReview",
            json!({"target":target,"requestKey":"start-remote"}),
            Some(&actor),
        )["review"]
            .clone();
        let scope = json!({"reviewId":review["id"]});
        let descriptor = fixture.rpc(
            "GetDiff",
            json!({"target":target,"comparison":review["comparison"]}),
            None,
        )["diff"]
            .clone();
        let metadata = fixture.rpc("GetFiles", json!({"diff":descriptor}), None);
        assert!(!metadata["files"].as_array().unwrap().is_empty());
        let selected = fixture.rpc(
            "GetFile",
            json!({"diff":descriptor,"path":"review.txt"}),
            None,
        );
        let old = selected["oldSide"]["content"].as_str().unwrap();
        let new = selected["newSide"]["content"].as_str().unwrap();
        assert!(old.contains("old") && new.contains("new"));
        assert!(
            !fixture.home.path().join(".git").exists(),
            "remote browsing needs no checkout"
        );
        let lines: Vec<_> = new.lines().collect();
        let line = json!({"kind":"line","path":"review.txt","start":"2","end":"2","side":"new",
            "source":selected["newSide"]["source"],"evidence":{"selectedText":lines[1],"contextBefore":lines[..1].join("\n"),"contextAfter":lines[2..].join("\n")}});
        let file = json!({"kind":"file","path":"review.txt","source":selected["newSide"]["source"],"evidence":{}});
        for (index, (body, anchor)) in [
            ("General remote concern", json!({"kind":"general"})),
            ("Inline remote concern", line),
            ("File remote concern", file),
        ]
        .into_iter()
        .enumerate()
        {
            let current = fixture.rpc("GetConversation", json!({"scope":scope}), None);
            fixture.rpc("AddComment",json!({"scope":scope,"body":body,"anchor":anchor,"expectedVersion":current["version"],"requestKey":format!("draft-{index}")}),Some(&actor));
        }
        let pending = fixture.remote.status();
        assert_eq!(
            pending["reviews"][0]["state"], "PENDING",
            "draft saves must not submit the review"
        );
        let writes_before_submit = pending["writes"].clone();
        let current =
            fixture.rpc("GetReview", json!({"reviewId":review["id"]}), None)["review"].clone();
        let input = json!({"reviewId":review["id"],"outcome":"approve","body":"Final E2E body","requestKey":"remote-once",
            "expectedHead":head,"expectedBase":base,"expectedVersion":current["version"]});
        fixture.remote.mode(mode);
        if mode == "stale" {
            assert!(
                !fixture
                    .response("SubmitReview", input, Some(&actor))
                    .0
                    .is_success()
            );
            assert_eq!(fixture.remote.status()["writes"], writes_before_submit);
            assert_eq!(
                fixture.rpc("GetReview", json!({"reviewId":review["id"]}), None)["review"]["state"],
                "active"
            );
            assert_eq!(
                fixture.rpc("GetConversation", json!({"scope":scope}), None)["comments"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            continue;
        }
        let submitted = fixture.rpc("SubmitReview", input.clone(), Some(&actor));
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
                .contains("Final E2E body")
        );
        fixture.remote.mode("normal");
        if mode == "lost" {
            fixture.restart();
        }
        let recovered = fixture.rpc(
            "ReconcileSubmission",
            json!({"reviewId":review["id"]}),
            Some(&actor),
        );
        assert_eq!(recovered["review"]["submissionStatus"], "confirmed");
        fixture.rpc("SubmitReview", input, Some(&actor));
        assert_eq!(fixture.remote.status()["writes"], before["writes"]);
        assert_eq!(fixture.remote.status()["errors"], json!([]));
        let exported = fixture.rpc("Export", json!({"scope":scope}), None);
        let markdown = exported["markdown"].as_str().unwrap();
        assert!(markdown.contains("Final E2E body") && markdown.contains("Inline remote concern"));
    }
}

#[test]
fn should_preserve_drafts_when_github_permission_is_denied() {
    let fixture = Backend::start();
    reqwest::blocking::Client::new()
        .post(format!("{}/__control", fixture.remote.endpoint_url))
        .json(&json!({"base":"a".repeat(40),"head":"b".repeat(40)}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();
    let actor = fixture.rpc(
        "EnsureIdentity",
        json!({"name":"Permission reviewer","email":"permission@example.invalid"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let review = fixture.rpc("StartReview",json!({"target":{"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}},"requestKey":"permission-start"}),Some(&actor))["review"].clone();
    let scope = json!({"reviewId":review["id"]});
    fixture.rpc("AddComment",json!({"scope":scope,"body":"Retain after denial","expectedVersion":review["version"],"requestKey":"permission-draft"}),Some(&actor));
    let before = fixture.rpc("GetConversation", json!({"scope":scope}), None);
    fixture.remote.mode("denied");
    let (status, error) = fixture.response(
        "RefreshReview",
        json!({"scope":scope,"expectedVersion":before["version"],"requestKey":"denied-refresh"}),
        Some(&actor),
    );
    assert!(!status.is_success());
    assert!(error["message"].is_string());
    let retained = fixture.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(retained["comments"][0]["body"], "Retain after denial");
    assert_eq!(fixture.remote.status()["writes"], 0);
    fixture.remote.mode("normal");
    fixture.rpc("RefreshReview",json!({"scope":scope,"expectedVersion":retained["version"],"requestKey":"recovered-refresh"}),Some(&actor));
    assert_eq!(fixture.remote.status()["errors"], json!([]));
}

#[test]
fn should_review_a_pr_through_typed_scopes_and_recover_submission_after_restart() {
    let mut f = Backend::start();
    let base = "a".repeat(40);
    let head = "b".repeat(40);
    reqwest::blocking::Client::new()
        .post(format!("{}/__control", f.remote.endpoint_url))
        .json(&json!({"base":base,"head":head}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();
    let actor = f.rpc(
        "EnsureIdentity",
        json!({"name":"Typed reviewer","email":"typed@example.test"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let start = json!({"target":{"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}},"requestKey":"start-pr"});
    let review = f.rpc("StartReview", start.clone(), Some(&actor))["review"].clone();
    let scope = json!({"reviewId":review["id"]});
    assert_eq!(review["comparison"]["pullRequest"]["baseCommit"], base);
    assert_eq!(review["forgeState"], "open");
    assert_eq!(
        f.remote.status()["content_reads"],
        0,
        "starting a PR review reads comments, not file bodies"
    );

    // An accepted start is recoverable even while the provider is unavailable.
    f.remote.mode("denied");
    assert_eq!(
        f.rpc("StartReview", start.clone(), Some(&actor))["review"]["id"],
        review["id"]
    );
    f.remote.mode("normal");
    let saved=f.rpc("AddComment",json!({"scope":scope,"body":"Remote file feedback","anchor":{"kind":"file","path":"review.txt"},"expectedVersion":review["version"],"requestKey":"file-feedback"}),Some(&actor));
    assert!(saved["syncError"].as_str().is_none_or(str::is_empty));
    assert_eq!(f.remote.status()["reviews"][0]["state"], "PENDING");
    let comment_id = &saved["comment"]["id"];
    let conversation = f.rpc("GetConversation", json!({"scope":scope}), None);
    let root = conversation["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|comment| &comment["id"] == comment_id)
        .unwrap();
    let edit = json!({"scope":scope,"commentId":comment_id,"body":"Edited remote feedback","expectedVersion":root["version"],"requestKey":"edit-feedback"});
    f.rpc("EditComment", edit.clone(), Some(&actor));
    let writes = f.remote.status()["writes"].clone();
    f.rpc("EditComment", edit, Some(&actor));
    assert_eq!(f.remote.status()["writes"], writes);
    let conversation = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(
        conversation["comments"][0]["body"],
        "Edited remote feedback"
    );
    let thread = &conversation["threads"][0];
    f.rpc("ResolveThread",json!({"scope":scope,"threadId":thread["id"],"resolved":true,"expectedVersion":thread["version"],"requestKey":"resolve-thread"}),Some(&actor));
    let conversation = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(conversation["threads"][0]["state"], "resolved");
    let reply=f.rpc("Reply",json!({"scope":scope,"threadId":thread["id"],"body":"Relevant reply","expectedVersion":conversation["threads"][0]["version"],"requestKey":"reply"}),Some(&actor));
    let conversation = f.rpc("GetConversation", json!({"scope":scope}), None);
    let reply = conversation["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == reply["comment"]["id"])
        .unwrap();
    let deletion = json!({"scope":scope,"commentId":reply["id"],"expectedVersion":reply["version"],"requestKey":"delete-reply"});
    f.rpc("DeleteComment", deletion.clone(), Some(&actor));
    let writes = f.remote.status()["writes"].clone();
    f.rpc("DeleteComment", deletion, Some(&actor));
    assert_eq!(f.remote.status()["writes"], writes);

    // A stale submit cannot freeze the attempt or publish any draft.
    let stale = json!({"reviewId":review["id"],"outcome":"approve","body":"Final typed feedback","requestKey":"stale-submit","expectedVersion":"0","expectedBase":base,"expectedHead":head});
    assert!(
        !f.response("SubmitReview", stale, Some(&actor))
            .0
            .is_success()
    );
    assert_eq!(f.remote.status()["writes"], writes);
    let current = f.rpc("GetReview", json!({"reviewId":review["id"]}), None);
    assert_eq!(current["review"]["state"], "active");
    let submit = json!({"reviewId":review["id"],"outcome":"approve","body":"Final typed feedback","requestKey":"submit-once","expectedVersion":current["review"]["version"],"expectedBase":base,"expectedHead":head});
    f.remote.mode("lost");
    assert_eq!(
        f.rpc("SubmitReview", submit.clone(), Some(&actor))["review"]["submissionStatus"],
        "uncertain"
    );
    let writes = f.remote.status()["writes"].clone();
    f.restart();
    f.remote.mode("normal");
    assert_eq!(
        f.rpc(
            "ReconcileSubmission",
            json!({"reviewId":review["id"]}),
            Some(&actor)
        )["review"]["submissionStatus"],
        "confirmed"
    );
    f.rpc("SubmitReview", submit, Some(&actor));
    assert_eq!(f.remote.status()["writes"], writes);
    assert!(
        f.rpc("Export", json!({"scope":scope}), None)["markdown"]
            .as_str()
            .unwrap()
            .contains("Final typed feedback")
    );
    assert_eq!(f.remote.status()["errors"], json!([]));
}

#[test]
fn should_keep_pr_feedback_readable_after_close_merge_and_reopen_without_completing_the_attempt() {
    let f = Backend::start();
    let actor = f.rpc(
        "EnsureIdentity",
        json!({"name":"Lifecycle reviewer","email":"lifecycle@example.test"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = json!({"target":{"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}},"requestKey":"start"});
    let review = f.rpc("StartReview", request.clone(), Some(&actor))["review"].clone();
    let scope = json!({"reviewId":review["id"]});
    f.rpc("AddComment",json!({"scope":scope,"body":"Retained lifecycle feedback","expectedVersion":review["version"],"requestKey":"general"}),Some(&actor));
    for lifecycle in ["closed", "merged", "normal"] {
        f.remote.mode(lifecycle);
        let current = f.rpc("GetReview", json!({"reviewId":review["id"]}), None);
        let refreshed = f.rpc("RefreshReview", json!({"scope":scope,"expectedVersion":current["review"]["version"],"requestKey":format!("refresh-{lifecycle}")}), Some(&actor));
        assert_eq!(
            refreshed["review"]["forgeState"],
            if lifecycle == "normal" {
                "open"
            } else {
                lifecycle
            }
        );
        assert_eq!(refreshed["review"]["state"], "active");
        assert_eq!(
            f.rpc("StartReview", request.clone(), Some(&actor))["review"]["id"],
            review["id"]
        );
        assert!(
            f.rpc("Export", json!({"scope":scope}), None)["markdown"]
                .as_str()
                .unwrap()
                .contains("Retained lifecycle feedback")
        );
    }
}

#[test]
fn should_promote_agent_notes_as_remotely_saved_drafts_owned_by_the_promoter() {
    let f = Backend::start();
    let author = f.rpc(
        "EnsureIdentity",
        json!({"name":"Review agent","model":"test-model","provider":"test-provider"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let promoter = f.rpc(
        "EnsureIdentity",
        json!({"name":"Promoter","email":"promoter@example.test"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let file = f.home.path().join("notes.md");
    std::fs::write(&file, "Saved notes\n").unwrap();
    let source = json!({"notes":{"path":file}});
    let note=f.rpc("AddComment",json!({"scope":source,"body":"Agent feedback to review","requestKey":"agent-note","expectedVersion":"0"}),Some(&author));
    let review=f.rpc("StartReview",json!({"target":{"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}},"requestKey":"start"}),Some(&promoter))["review"].clone();
    let source_notes = f.rpc("GetConversation", json!({"scope":source}), None);
    let request = json!({"source":source,"targetReviewId":review["id"],"commentIds":[note["comment"]["id"]],"anchors":[{"kind":"file","path":"review.txt"}],"expectedSourceVersion":source_notes["version"],"expectedReviewVersion":review["version"],"requestKey":"promote-agent-note"});
    let promoted = f.rpc("PromoteNotes", request.clone(), Some(&promoter));
    assert!(
        promoted["syncErrors"]
            .as_array()
            .is_none_or(|errors| errors.is_empty())
    );
    assert_eq!(promoted["comments"][0]["author"]["id"], author);
    assert_eq!(promoted["comments"][0]["ownerId"], promoter);
    assert_eq!(promoted["comments"][0]["postingStatus"], "confirmed");
    let status = f.remote.status();
    assert!(
        status["threads"]["1"][0]["comments"]["nodes"][0]["body"]
            .as_str()
            .unwrap()
            .contains("Review agent")
    );
    f.rpc("PromoteNotes", request, Some(&promoter));
    assert_eq!(f.remote.status()["writes"], status["writes"]);
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":source}), None)["comments"][0]["body"],
        "Agent feedback to review"
    );
}

#[test]
fn should_validate_pr_selection_bytes_locally_or_with_only_selected_remote_content_and_retry_without_the_source()
 {
    use sha2::{Digest, Sha256};
    for local_checkout in [false, true] {
        let f = Backend::start();
        let root = f.home.path().join("checkout");
        std::fs::create_dir(&root).unwrap();
        let git = |args: &[&str]| {
            let result = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout).unwrap().trim().to_owned()
        };
        let bytes = b"one\nnew\nthree\n";
        let (base, head) = if local_checkout {
            git(&["init", "-q"]);
            std::fs::write(root.join("review.txt"), "one\nold\nthree\n").unwrap();
            git(&["add", "review.txt"]);
            git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-qm",
                "base",
            ]);
            let base = git(&["rev-parse", "HEAD"]);
            std::fs::write(root.join("review.txt"), bytes).unwrap();
            git(&["add", "review.txt"]);
            git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-qm",
                "head",
            ]);
            let head = git(&["rev-parse", "HEAD"]);
            git(&["checkout", "--detach", "-q", &head]);
            (base, head)
        } else {
            ("a".repeat(40), "b".repeat(40))
        };
        reqwest::blocking::Client::new()
            .post(format!("{}/__control", f.remote.endpoint_url))
            .json(&json!({"base":base,"head":head}))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        let actor = f.rpc(
            "EnsureIdentity",
            json!({"name":"Source reviewer","email":"source@example.test"}),
            None,
        )["actor"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut start = json!({"target":{"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}},"requestKey":"start"});
        if local_checkout {
            start["localGitPath"] = json!(root);
        }
        let review = f.rpc("StartReview", start, Some(&actor))["review"].clone();
        let scope = json!({"reviewId":review["id"]});
        let anchor = json!({"kind":"line","path":"review.txt","side":"new","start":"2","end":"2","source":{"commit":{"sha":head,"contentHash":format!("sha256:{:x}",Sha256::digest(bytes))}},"evidence":{"selectedText":"new","contextBefore":"one","contextAfter":"three"}});
        let mut incorrect = anchor.clone();
        incorrect["source"]["commit"]["contentHash"] = json!(format!("sha256:{}", "0".repeat(64)));
        assert!(!f.response("AddComment",json!({"scope":scope,"anchor":incorrect,"body":"Wrong source","expectedVersion":review["version"],"requestKey":"wrong-source"}),Some(&actor)).0.is_success());
        let untouched = f.rpc("GetConversation", json!({"scope":scope}), None);
        assert_eq!(untouched["version"], review["version"]);
        assert!(
            untouched["comments"]
                .as_array()
                .is_none_or(|comments| comments.is_empty())
        );
        let request = json!({"scope":scope,"anchor":anchor,"body":"Exact selected source","expectedVersion":review["version"],"requestKey":"source-comment"});
        let saved = f.rpc("AddComment", request.clone(), Some(&actor));
        assert!(saved["syncError"].as_str().is_none_or(str::is_empty));
        assert_eq!(saved["comment"]["postingStatus"], "confirmed");
        let before = f.remote.status();
        assert_eq!(
            before["content_reads"],
            if local_checkout { 0 } else { 2 },
            "local validation downloads zero bodies; remote validation reuses only verified selected-file bytes after rejecting the wrong hash"
        );
        std::fs::remove_dir_all(&root).unwrap();
        f.remote.mode("denied");
        let retried = f.rpc("AddComment", request, Some(&actor));
        assert_eq!(retried["comment"]["id"], saved["comment"]["id"]);
        assert!(
            retried["syncError"].as_str().is_none_or(str::is_empty),
            "a confirmed receipt does not need another provider write or read"
        );
        assert_eq!(f.remote.status()["writes"], before["writes"]);
        assert_eq!(f.remote.status()["content_reads"], before["content_reads"]);
    }
}

#[test]
fn should_explicitly_accept_changed_pr_comparisons_and_retain_feedback_without_recreating_provider_drafts()
 {
    let f = Backend::start();
    let control = |input: Value| {
        reqwest::blocking::Client::new()
            .post(format!("{}/__control", f.remote.endpoint_url))
            .json(&input)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
    };
    let base = "a".repeat(40);
    let head = "b".repeat(40);
    control(json!({"base":base,"head":head}));
    let actor = f.rpc(
        "EnsureIdentity",
        json!({"name":"Comparison reviewer","email":"comparison@example.test"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let target =
        json!({"pullRequest":{"repositoryUrl":"https://github.com/acme/repo","number":"1"}});
    let first = f.rpc(
        "StartReview",
        json!({"target":target,"requestKey":"first"}),
        Some(&actor),
    )["review"]
        .clone();
    let scope = json!({"reviewId":first["id"]});
    let note=f.rpc("AddComment",json!({"scope":scope,"body":"Retained private feedback","expectedVersion":first["version"],"requestKey":"private-general"}),Some(&actor));
    control(json!({"head":"c".repeat(40)}));
    assert!(
        !f.response(
            "StartReview",
            json!({"target":target,"requestKey":"implicit-switch"}),
            Some(&actor)
        )
        .0
        .is_success()
    );
    let current = f.rpc("GetReview", json!({"reviewId":first["id"]}), None);
    let acceptance = json!({"target":target,"resumePolicy":"RESUME_POLICY_ACCEPT_PR_COMPARISON","expectedReviewId":first["id"],"expectedVersion":current["review"]["version"],"requestKey":"accept-c"});
    let second = f.rpc("StartReview", acceptance.clone(), Some(&actor))["review"].clone();
    assert_ne!(second["id"], first["id"]);
    assert_eq!(
        second["comparison"]["pullRequest"]["headCommit"],
        "c".repeat(40)
    );
    let prior = f.rpc("GetReview", json!({"reviewId":first["id"]}), None);
    assert_eq!(prior["review"]["outcome"], "superseded");
    let shared = f.rpc(
        "GetConversation",
        json!({"scope":{"reviewId":second["id"]}}),
        None,
    );
    assert_eq!(shared["comments"][0]["id"], note["comment"]["id"]);
    assert_eq!(shared["comments"][0]["visibility"], "draft");
    assert_eq!(
        f.remote.status()["writes"],
        0,
        "acceptance does not publish retained drafts"
    );
    f.remote.mode("denied");
    assert_eq!(
        f.rpc("StartReview", acceptance, Some(&actor))["review"]["id"],
        second["id"]
    );
    f.remote.mode("normal");

    let saved=f.rpc("AddComment",json!({"scope":{"reviewId":second["id"]},"body":"Pending feedback to preserve","anchor":{"kind":"file","path":"review.txt"},"expectedVersion":shared["version"],"requestKey":"pending-file"}),Some(&actor));
    let writes = f.remote.status()["writes"].clone();
    control(json!({"head":"d".repeat(40)}));
    let current = f.rpc("GetReview", json!({"reviewId":second["id"]}), None);
    let blocked = json!({"target":target,"resumePolicy":"RESUME_POLICY_ACCEPT_PR_COMPARISON","expectedReviewId":second["id"],"expectedVersion":current["review"]["version"],"requestKey":"pending-blocked"});
    assert!(
        !f.response("StartReview", blocked, Some(&actor))
            .0
            .is_success()
    );
    assert_eq!(
        f.rpc("GetReview", json!({"reviewId":second["id"]}), None)["review"]["state"],
        "active"
    );
    assert_eq!(f.remote.status()["writes"], writes);

    // Discard happens outside LGTM. Acceptance preserves its local discussion without recreating it.
    control(json!({"discard_pending":true}));
    let current = f.rpc("GetReview", json!({"reviewId":second["id"]}), None);
    let third=f.rpc("StartReview",json!({"target":target,"resumePolicy":"RESUME_POLICY_ACCEPT_PR_COMPARISON","expectedReviewId":second["id"],"expectedVersion":current["review"]["version"],"requestKey":"accept-d"}),Some(&actor))["review"].clone();
    assert_ne!(third["id"], second["id"]);
    assert_eq!(f.remote.status()["writes"], writes);
    assert!(f.remote.status()["reviews"].as_array().unwrap().is_empty());
    let retained = f.rpc(
        "GetConversation",
        json!({"scope":{"reviewId":third["id"]}}),
        None,
    );
    let pending = retained["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|comment| comment["id"] == saved["comment"]["id"])
        .unwrap();
    assert_eq!(pending["body"], "Pending feedback to preserve");
    assert_eq!(pending["remotePresence"], "missing");
    assert_eq!(pending["visibility"], "draft");
}
