//! Comparison replacement and resume choices.
use super::ComparisonKind;
use serde::{Deserialize, Serialize};

/// Changing comparisons requires explicit acceptance of the selected prior attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartOptions {
    pub comparison_kind: ComparisonKind,
    pub replace_local: bool,
    pub accept_pr_comparison: bool,
    pub expected_active_id: Option<uuid::Uuid>,
}

impl From<ComparisonKind> for StartOptions {
    fn from(comparison_kind: ComparisonKind) -> Self {
        Self {
            comparison_kind,
            replace_local: false,
            accept_pr_comparison: false,
            expected_active_id: None,
        }
    }
}
