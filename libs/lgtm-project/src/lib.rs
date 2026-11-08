//! Project checkout resolution and managed/external Git worktree management for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmProjectError;
pub use error::Result;
pub use models::project::Project;
pub use models::project::Worktree;
pub use models::project::WorktreeSource;
pub use services::project::ProjectService;
