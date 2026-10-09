//! `TestEnv` — one running `lgtm-server` subprocess with its own temp db.
//!
//! Tests get an ergonomic façade on top: `env.list_files(repo, base)` instead
//! of hand-rolling an HTTP POST. The façade uses blocking reqwest; tests
//! aren't performance-critical and sync code reads cleaner.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Serialize, de::DeserializeOwned};
use tempfile::TempDir;

use lgtm_grpc::proto::*;

use crate::repo::GitRepoFixture;

const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve the compiled `lgtm-server` binary that lives alongside the test
/// binary. `cargo test` places both under `target/<profile>/`.
pub fn lgtm_server_binary() -> PathBuf {
    // tests run from the crate dir but `CARGO_BIN_EXE_<name>` isn't set for
    // workspace bin deps; walk up from the test binary location.
    let exe = std::env::current_exe().expect("current_exe");
    // target/debug/deps/<test>-<hash>  → target/debug/
    let dir = exe.parent().and_then(|p| p.parent()).expect("profile dir");
    dir.join("lgtm-server")
}

pub fn lgtm_cli_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("profile dir");
    dir.join("lgtm")
}

pub struct TestEnv {
    // Keep the TempDir alive for the life of the env; dropping it removes
    // the sqlite file.
    _db_dir: TempDir,
    base_url: String,
    client: Client,
    server: Child,
}

impl TestEnv {
    pub fn spawn() -> Self {
        let bin = lgtm_server_binary();
        if !bin.exists() {
            panic!(
                "lgtm-server binary not found at {}. Run `cargo build -p lgtm-server` first.",
                bin.display()
            );
        }

        let db_dir = tempfile::tempdir().expect("tempdir for db");
        let db_path = db_dir.path().join("db.sqlite");

        let port = pick_free_port();
        let addr = format!("127.0.0.1:{port}");

        let server = Command::new(&bin)
            .env("LGTM_ADDR", &addr)
            .env("LGTM_DB_PATH", &db_path)
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));

        let base_url = format!("http://{addr}");
        let client = Client::new();

        wait_ready(&client, &base_url).unwrap_or_else(|e| panic!("server not ready: {e:#}"));

        Self { _db_dir: db_dir, base_url, client, server }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // ----- Ergonomic façades -----

    pub fn seed_repo(&self) -> GitRepoFixture {
        GitRepoFixture::standard()
    }

    pub fn list_files(&self, repo: &GitRepoFixture, base: &str) -> Vec<ChangedFile> {
        self.unary::<_, GetChangedFilesResponse>(
            "GetChangedFiles",
            &GetChangedFilesRequest {
                repo_path: repo.path().to_string_lossy().into_owned(),
                base_ref: base.into(),
            },
        )
        .files
    }

    pub fn file_diff(&self, repo: &GitRepoFixture, base: &str, path: &str) -> String {
        self.unary::<_, GetFileDiffResponse>(
            "GetFileDiff",
            &GetFileDiffRequest {
                repo_path: repo.path().to_string_lossy().into_owned(),
                path: path.into(),
                base_ref: base.into(),
            },
        )
        .diff_text
    }

    pub fn upsert_user(&self, name: &str, kind: AuthorKind) -> User {
        self.unary::<_, UpsertUserResponse>(
            "UpsertUser",
            &UpsertUserRequest { name: name.into(), kind },
        )
        .user
    }

    pub fn create_thread(
        &self,
        repo: &GitRepoFixture,
        base: &str,
        file: &str,
        line: i32,
        author_id: i64,
        body: &str,
    ) -> Thread {
        self.unary::<_, CreateThreadResponse>(
            "CreateThread",
            &CreateThreadRequest {
                repo_path: repo.path().to_string_lossy().into_owned(),
                base_ref: base.into(),
                file_path: file.into(),
                line,
                author_id,
                body: body.into(),
            },
        )
        .thread
    }

    pub fn append_comment(&self, thread_id: i64, author_id: i64, body: &str) -> Comment {
        self.unary::<_, AppendCommentResponse>(
            "AppendComment",
            &AppendCommentRequest { thread_id, author_id, body: body.into() },
        )
        .comment
    }

    pub fn resolve_thread(&self, thread_id: i64) -> Thread {
        self.unary::<_, ResolveThreadResponse>(
            "ResolveThread",
            &ResolveThreadRequest { thread_id },
        )
        .thread
    }

    pub fn list_threads(
        &self,
        repo: &GitRepoFixture,
        base: &str,
    ) -> Vec<Thread> {
        self.list_threads_full(repo, base, None, false)
    }

    pub fn list_threads_full(
        &self,
        repo: &GitRepoFixture,
        base: &str,
        file: Option<&str>,
        only_unresolved: bool,
    ) -> Vec<Thread> {
        self.unary::<_, ListThreadsResponse>(
            "ListThreads",
            &ListThreadsRequest {
                repo_path: repo.path().to_string_lossy().into_owned(),
                base_ref: base.into(),
                file_path: file.unwrap_or("").into(),
                only_unresolved,
            },
        )
        .threads
    }

    // ----- Expect-error variants for negative tests -----

    pub fn try_create_thread(
        &self,
        repo: &GitRepoFixture,
        base: &str,
        file: &str,
        line: i32,
        author_id: i64,
        body: &str,
    ) -> Result<Thread> {
        self.try_unary::<_, CreateThreadResponse>(
            "CreateThread",
            &CreateThreadRequest {
                repo_path: repo.path().to_string_lossy().into_owned(),
                base_ref: base.into(),
                file_path: file.into(),
                line,
                author_id,
                body: body.into(),
            },
        )
        .map(|r| r.thread)
    }

    // ----- Internal -----

    fn unary<Req: Serialize, Res: DeserializeOwned>(&self, method: &str, req: &Req) -> Res {
        self.try_unary(method, req)
            .unwrap_or_else(|e| panic!("{method} failed: {e:#}"))
    }

    fn try_unary<Req: Serialize, Res: DeserializeOwned>(
        &self,
        method: &str,
        req: &Req,
    ) -> Result<Res> {
        let url = format!("{}/lgtm.v1.LgtmService/{method}", self.base_url);
        let resp = self.client.post(&url).json(req).send().with_context(|| format!("POST {url}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().unwrap_or(serde_json::json!({}));
            let msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("<no message>");
            bail!("{method} {status}: {msg}");
        }
        resp.json::<Res>().with_context(|| format!("decode {method}"))
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

fn wait_ready(client: &Client, base_url: &str) -> Result<()> {
    let deadline = Instant::now() + READY_TIMEOUT;
    // Probe UpsertUser — it's the fastest endpoint with no filesystem side effects.
    let url = format!("{base_url}/lgtm.v1.LgtmService/UpsertUser");
    while Instant::now() < deadline {
        let probe = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(r#"{"name":"__probe","kind":"AUTHOR_KIND_AGENT"}"#)
            .send();
        if let Ok(r) = probe {
            if r.status().is_success() {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!("server at {base_url} not ready after {:?}", READY_TIMEOUT);
}
