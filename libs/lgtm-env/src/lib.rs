//! Managed Mise binary installation, `.lgtm/mise.toml` synthesis, and task/tool execution for LGTM workspaces.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmEnvError;
pub use error::Result;
pub use models::mise::GeneratedMiseFile;
pub use models::mise::MiseTaskEntry;
pub use models::mise::MiseTaskSummary;
pub use services::mise::MiseService;
