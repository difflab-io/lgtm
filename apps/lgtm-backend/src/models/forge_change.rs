//! Saved edit/delete/resolve intent. Submission receipts remain on the existing review attempt.
use super::{Comment, ObjectKind, Thread};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ForgeChangeInput {
    Edit { comment_id: Uuid, body: String },
    Delete { comment_id: Uuid },
    Resolve { thread_id: Uuid, resolved: bool },
}

impl ForgeChangeInput {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Edit { .. } => "remote_edit_comment",
            Self::Delete { .. } => "remote_delete_comment",
            Self::Resolve { .. } => "remote_resolve_thread",
        }
    }

    pub fn object(&self) -> (ObjectKind, Uuid) {
        match self {
            Self::Edit { comment_id, .. } | Self::Delete { comment_id } => {
                (ObjectKind::Comment, *comment_id)
            }
            Self::Resolve { thread_id, .. } => (ObjectKind::Thread, *thread_id),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ForgeChangeObject {
    Deleted { comment_id: Uuid },
    Comment(Box<Comment>),
    Thread(Box<Thread>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeChange {
    pub input: ForgeChangeInput,
    pub original: ForgeChangeObject,
    /// Only populated after an authoritative observation confirms the saved intent.
    pub result: Option<ForgeChangeObject>,
    /// A definite provider rejection can release the object without replaying this intent.
    pub rejection: Option<String>,
    /// Identity-only deletion receipt prevents stale provider pages from restoring deleted feedback.
    #[serde(default)]
    pub deleted_remote: Option<(String, String)>,
}
