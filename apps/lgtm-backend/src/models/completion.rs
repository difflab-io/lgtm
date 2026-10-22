use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Completion {
    pub outcome: Option<String>,
    pub body: Option<String>,
    pub submit: bool,
}
