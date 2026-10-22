//! General time, path and serialization helpers.
use serde::Serialize;
use std::path::Path;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn within(path: &str, root: &str) -> bool {
    Path::new(path).starts_with(Path::new(root))
}

/// Hash a domain response without relying on timestamps to distinguish concurrent edits.
pub fn object_hash<T: Serialize>(value: &T) -> crate::error::Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
