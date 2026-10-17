use super::{Anchor, RemoteComment};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteThread {
    pub id: String,
    pub anchor: Anchor,
    pub resolved: bool,
    pub comments: Vec<RemoteComment>,
}
