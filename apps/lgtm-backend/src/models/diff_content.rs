//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use super::{ContentSide, DiffFile};

#[derive(Clone, Debug)]
pub struct DiffContent {
    pub file: DiffFile,
    pub old: Option<ContentSide>,
    pub new: Option<ContentSide>,
}
