//! Object discriminator in concurrency tokens.
use serde::{Deserialize, Serialize};

/// Object tokens bind a scope revision to the exact comment or thread that a client read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Comment,
    Thread,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Thread => "thread",
        }
    }
}
