use super::RemoteThread;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeSnapshot {
    pub complete: bool,
    pub head: String,
    pub base: Option<String>,
    pub threads: Vec<RemoteThread>,
}
