//! Serde message types matching `proto/lgtm.proto`.
//!
//! Field names use `snake_case` because that is Connect's canonical JSON
//! encoding for proto field names. Keep this file in sync with the .proto
//! source of truth.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetChangedFilesRequest {
    pub repo_path: String,
    pub base_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetChangedFilesResponse {
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFileDiffRequest {
    pub repo_path: String,
    pub path: String,
    pub base_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFileDiffResponse {
    pub diff_text: String,
}
