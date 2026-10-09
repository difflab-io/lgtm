//! Shared fixtures for the lgtm end-to-end tests.
//!
//! Design goals (per repo guidance): tests should read declaratively, so this
//! crate hides the plumbing (port selection, server spawn, db isolation,
//! git-repo seeding) behind small named helpers:
//!
//! ```ignore
//! let env = TestEnv::spawn();
//! let repo = env.seed_repo();
//! let agent = env.upsert_user("claude", AuthorKind::Agent);
//! let thread = env.create_thread(&repo, "HEAD~1", "src/lib.rs", 3, agent.id, "nit");
//! ```
//!
//! Each `TestEnv` runs its own `lgtm-server` subprocess bound to a port the
//! OS picks (via `127.0.0.1:0`), with its own temp SQLite path. The server is
//! killed when the env is dropped.

pub mod env;
pub mod repo;

pub use env::{TestEnv, lgtm_server_binary};
pub use repo::GitRepoFixture;

// Re-export proto types for ergonomic test use.
pub use lgtm_grpc::proto::{
    AppendCommentRequest, AppendCommentResponse, AuthorKind, ChangedFile, Comment,
    CreateThreadRequest, CreateThreadResponse, GetChangedFilesRequest, GetChangedFilesResponse,
    GetFileDiffRequest, GetFileDiffResponse, ListThreadsRequest, ListThreadsResponse,
    ResolveThreadRequest, ResolveThreadResponse, Thread, UpsertUserRequest, UpsertUserResponse,
    User,
};
