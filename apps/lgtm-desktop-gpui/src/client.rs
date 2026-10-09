//! Blocking HTTP client for the lgtm Connect-JSON server.
//!
//! Blocking is intentional — the GPUI app shells out to the client from
//! `cx.background_spawn` closures that already give us a worker thread, so
//! we don't need a tokio runtime inside the UI process.

use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};

use crate::proto::*;

#[derive(Clone)]
pub struct LgtmClient {
    base_url: String,
    agent: ureq::Agent,
}

impl LgtmClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), agent: ureq::agent() }
    }

    pub fn upsert_user(&self, name: &str, kind: AuthorKind) -> Result<User> {
        self.unary::<_, UpsertUserResponse>(
            "UpsertUser",
            &UpsertUserRequest { name: name.into(), kind },
        )
        .map(|r| r.user)
    }

    pub fn get_changed_files(&self, repo: &str, base: &str) -> Result<Vec<ChangedFile>> {
        self.unary::<_, GetChangedFilesResponse>(
            "GetChangedFiles",
            &GetChangedFilesRequest { repo_path: repo.into(), base_ref: base.into() },
        )
        .map(|r| r.files)
    }

    pub fn get_file_diff(&self, repo: &str, base: &str, path: &str) -> Result<String> {
        self.unary::<_, GetFileDiffResponse>(
            "GetFileDiff",
            &GetFileDiffRequest {
                repo_path: repo.into(),
                path: path.into(),
                base_ref: base.into(),
            },
        )
        .map(|r| r.diff_text)
    }

    pub fn list_threads(&self, repo: &str, base: &str, file: Option<&str>) -> Result<Vec<Thread>> {
        self.unary::<_, ListThreadsResponse>(
            "ListThreads",
            &ListThreadsRequest {
                repo_path: repo.into(),
                base_ref: base.into(),
                file_path: file.unwrap_or("").into(),
                only_unresolved: false,
            },
        )
        .map(|r| r.threads)
    }

    pub fn create_thread(
        &self,
        repo: &str,
        base: &str,
        file: &str,
        line: i32,
        author_id: i64,
        body: &str,
    ) -> Result<Thread> {
        self.unary::<_, CreateThreadResponse>(
            "CreateThread",
            &CreateThreadRequest {
                repo_path: repo.into(),
                base_ref: base.into(),
                file_path: file.into(),
                line,
                author_id,
                body: body.into(),
            },
        )
        .map(|r| r.thread)
    }

    pub fn append_comment(&self, thread_id: i64, author_id: i64, body: &str) -> Result<Comment> {
        self.unary::<_, AppendCommentResponse>(
            "AppendComment",
            &AppendCommentRequest { thread_id, author_id, body: body.into() },
        )
        .map(|r| r.comment)
    }

    pub fn resolve_thread(&self, thread_id: i64) -> Result<Thread> {
        self.unary::<_, ResolveThreadResponse>(
            "ResolveThread",
            &ResolveThreadRequest { thread_id },
        )
        .map(|r| r.thread)
    }

    fn unary<Req: Serialize, Res: DeserializeOwned>(
        &self,
        method: &str,
        req: &Req,
    ) -> Result<Res> {
        let url = format!("{}/lgtm.v1.LgtmService/{method}", self.base_url);
        match self.agent.post(&url).send_json(req) {
            Ok(resp) => resp
                .into_json::<Res>()
                .with_context(|| format!("decode {method} response")),
            Err(ureq::Error::Status(code, resp)) => {
                let body: serde_json::Value = resp.into_json().unwrap_or_default();
                let msg = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<no message>");
                Err(anyhow!("{method} failed ({code}): {msg}"))
            }
            Err(e) => Err(anyhow!("{method} transport error: {e}")),
        }
    }
}
