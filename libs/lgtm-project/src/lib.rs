//! Project checkout resolution and managed Git worktree management for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmProjectError;
pub use error::Result;
pub use models::project::Project;
pub use models::worktree::Worktree;
pub use services::project::ProjectService;
