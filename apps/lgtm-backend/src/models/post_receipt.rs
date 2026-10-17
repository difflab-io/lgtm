use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostReceipt {
    pub comment_id: String,
    pub kind: String,
    pub thread_id: Option<String>,
    pub review_id: Option<String>,
    pub posting_account_id: String,
}
