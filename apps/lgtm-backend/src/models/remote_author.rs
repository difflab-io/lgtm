use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteAuthor {
    pub name: String,
    pub email: String,
    pub account_id: String,
}
