//! Blocking reqwest wrapper over the Connect-JSON unary endpoints.
//!
//! Blocking is deliberate — the CLI is single-shot and avoiding tokio keeps
//! the binary small and the error reporting simple.

use anyhow::{Context, Result, bail};
use lgtm_grpc::proto::*;
use reqwest::blocking::Client;
use serde::{Serialize, de::DeserializeOwned};

pub struct LgtmClient {
    base_url: String,
    http: Client,
}

impl LgtmClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: Client::new(),
        }
    }

    pub fn get_changed_files(
        &self,
        req: &GetChangedFilesRequest,
    ) -> Result<GetChangedFilesResponse> {
        self.unary("GetChangedFiles", req)
    }

    pub fn get_file_diff(&self, req: &GetFileDiffRequest) -> Result<GetFileDiffResponse> {
        self.unary("GetFileDiff", req)
    }

    pub fn upsert_user(&self, req: &UpsertUserRequest) -> Result<UpsertUserResponse> {
        self.unary("UpsertUser", req)
    }

    pub fn list_threads(&self, req: &ListThreadsRequest) -> Result<ListThreadsResponse> {
        self.unary("ListThreads", req)
    }

    pub fn create_thread(&self, req: &CreateThreadRequest) -> Result<CreateThreadResponse> {
        self.unary("CreateThread", req)
    }

    pub fn append_comment(&self, req: &AppendCommentRequest) -> Result<AppendCommentResponse> {
        self.unary("AppendComment", req)
    }

    pub fn resolve_thread(&self, req: &ResolveThreadRequest) -> Result<ResolveThreadResponse> {
        self.unary("ResolveThread", req)
    }

    fn unary<Req, Res>(&self, method: &str, req: &Req) -> Result<Res>
    where
        Req: Serialize + ?Sized,
        Res: DeserializeOwned,
    {
        let url = format!("{}/lgtm.v1.LgtmService/{method}", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(req)
            .send()
            .with_context(|| format!("POST {url}: could not reach lgtm backend"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().unwrap_or_else(|_| serde_json::json!({}));
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("<no message>");
            bail!("lgtm {method} failed ({status}): {msg}");
        }
        resp.json::<Res>().with_context(|| format!("decode {method} response"))
    }
}
