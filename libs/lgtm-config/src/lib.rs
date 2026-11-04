//! Configuration and Mise task orchestration for LGTM workspaces.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmConfigError;
pub use error::Result;
pub use models::config::LanguageConfig;
pub use models::config::LgtmConfig;
pub use models::config::LspSettings;
pub use models::config::MiseSettings;
pub use models::config::MiseTaskConfig;
pub use models::mise::GeneratedMiseFile;
pub use models::mise::MiseTaskEntry;
pub use models::mise::MiseTaskSummary;
pub use services::config::ConfigService;
pub use services::mise::MiseService;
