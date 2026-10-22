use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitContext {
    pub base: Option<String>,
    pub head: Option<String>,
}
