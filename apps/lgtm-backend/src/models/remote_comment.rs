use super::RemoteAuthor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteComment {
    pub id: String,
    pub kind: String,
    pub review_id: Option<String>,
    pub body: String,
    pub updated_at: String,
    pub author: RemoteAuthor,
    pub reply_to: Option<String>,
}
