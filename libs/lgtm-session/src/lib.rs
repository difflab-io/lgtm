//! Workspace session orchestration and persistence (`~/.lgtm/sessions/<id>.json`) for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmSessionError;
pub use error::Result;
pub use models::session::Session;
pub use services::session::SessionService;
