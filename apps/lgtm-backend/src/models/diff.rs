//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use super::{AnchorSource, ReviewComparison, ReviewTarget};
use serde::{Deserialize, Serialize};

/// Transient content metadata. It is never persisted as a review record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffComparison {
    pub target: ReviewTarget,
    pub comparison: ReviewComparison,
    pub root: Option<String>,
    pub observation_token: String,
}

/// File metadata contains no downloaded file bodies.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub previous_path: String,
    pub status: String,
    pub identity: String,
}

#[derive(Clone, Debug)]
pub struct ContentSide {
    pub path: String,
    pub source: AnchorSource,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct DiffContent {
    pub file: DiffFile,
    pub old: Option<ContentSide>,
    pub new: Option<ContentSide>,
}
