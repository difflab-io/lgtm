use super::{Comment, Context, Session, Thread};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub context: Context,
    pub sessions: Vec<Session>,
    pub threads: Vec<Thread>,
    pub comments: Vec<Comment>,
}
