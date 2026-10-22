//! Input for publishing a review to its forge.

/// Final review intent, guarded by the comparison and conversation the consumer reviewed.
#[derive(Clone, Debug)]
pub struct ReviewSubmission {
    pub outcome: String,
    pub body: String,
    pub request_key: String,
    pub expected_head: String,
    pub expected_base: Option<String>,
    pub expected_version: Option<i64>,
}
