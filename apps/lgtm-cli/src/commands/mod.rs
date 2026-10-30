//! CLI command groups; domain behavior belongs to the backend.
pub mod comment;
pub mod commit;
pub mod push;
pub mod review;
pub mod ui;

/// Stable error for the intentionally unimplemented command surface.
#[derive(Debug, thiserror::Error)]
#[error("{operation} is not implemented yet; no checkout or forge changes were made")]
pub struct NotImplemented {
    pub operation: &'static str,
}
