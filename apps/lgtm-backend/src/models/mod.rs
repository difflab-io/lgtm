//! Domain models and command inputs, separate from SeaORM entity rows.
use super::error::{Result, invalid};
use std::path::Path;
pub mod context_key;
pub use context_key::ContextKey;
pub mod actor;
pub use actor::Actor;
pub mod context;
pub use context::Context;
pub mod commit_context;
pub use commit_context::CommitContext;
pub mod session;
pub use session::Session;
pub mod anchor;
pub use anchor::Anchor;
pub mod thread;
pub use thread::Thread;
pub mod comment;
pub use comment::Comment;
pub mod conversation;
pub use conversation::Conversation;
pub mod session_filter;
pub use session_filter::SessionFilter;
pub mod completion;
pub use completion::Completion;

pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn within(path: &str, root: &str) -> bool {
    Path::new(path).starts_with(Path::new(root))
}

pub mod remote_author;
pub use remote_author::RemoteAuthor;
pub mod remote_comment;
pub use remote_comment::RemoteComment;
pub mod remote_thread;
pub use remote_thread::RemoteThread;
pub mod forge_snapshot;
pub use forge_snapshot::ForgeSnapshot;
pub mod post_receipt;
pub use post_receipt::PostReceipt;

mod identities;
