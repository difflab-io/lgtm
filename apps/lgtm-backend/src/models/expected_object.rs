//! Selected object identity used to reject stale changes.
use super::ObjectKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedObject {
    pub kind: ObjectKind,
    pub id: uuid::Uuid,
    pub hash: String,
}

impl ExpectedObject {
    // Public ------------------------------------------------------------------

    /// Opaque to consumers. Saved source identity is separate from this review-data token.
    pub(crate) fn get_version<T: serde::Serialize>(
        kind: crate::models::ObjectKind,
        version: i64,
        id: uuid::Uuid,
        value: &T,
    ) -> crate::error::Result<String> {
        Ok(format!(
            "{}:{version}:{id}:{}",
            kind.as_str(),
            crate::utils::object_hash(value)?
        ))
    }
}
