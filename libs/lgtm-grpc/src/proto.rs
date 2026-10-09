//! Serde message types matching `proto/lgtm.proto`.
//!
//! Field names use `snake_case` because that is Connect's canonical JSON
//! encoding for proto field names. Enums serialise as their SCREAMING_SNAKE
//! string names, matching Connect's default JSON policy.
//!
//! Keep this file in lockstep with the .proto source of truth.

use serde::{Deserialize, Serialize};

// -------- Diff messages --------

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

// -------- Identity --------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorKind {
    #[serde(rename = "AUTHOR_KIND_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "AUTHOR_KIND_USER")]
    User,
    #[serde(rename = "AUTHOR_KIND_AGENT")]
    Agent,
}

impl AuthorKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Unspecified => "unspecified",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "user" => Self::User,
            "agent" => Self::Agent,
            _ => Self::Unspecified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub kind: AuthorKind,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertUserRequest {
    pub name: String,
    pub kind: AuthorKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertUserResponse {
    pub user: User,
}

// -------- Comments --------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub repo_path: String,
    pub base_ref: String,
    pub file_path: String,
    /// 1-indexed line in the *new* file; 0 means "whole file".
    pub line: i32,
    pub resolved: bool,
    pub created_at: i64,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub thread_id: i64,
    pub author: User,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListThreadsRequest {
    pub repo_path: String,
    pub base_ref: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub only_unresolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListThreadsResponse {
    pub threads: Vec<Thread>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreadRequest {
    pub repo_path: String,
    pub base_ref: String,
    pub file_path: String,
    pub line: i32,
    pub author_id: i64,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreadResponse {
    pub thread: Thread,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendCommentRequest {
    pub thread_id: i64,
    pub author_id: i64,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendCommentResponse {
    pub comment: Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveThreadRequest {
    pub thread_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveThreadResponse {
    pub thread: Thread,
}
