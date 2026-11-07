//! Configuration and language discovery for LGTM workspaces.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmConfigError;
pub use error::Result;
pub use models::config::AppearanceSettings;
pub use models::config::AppearanceSettingsPatch;
pub use models::config::DEFAULT_SCHEMA_URL;
pub use models::config::EditorSettings;
pub use models::config::EditorSettingsPatch;
pub use models::config::LanguageConfig;
pub use models::config::LgtmConfig;
pub use models::config::LgtmConfigPatch;
pub use models::config::LineWrap;
pub use models::config::LspSettings;
pub use models::config::LspSettingsPatch;
pub use models::config::MiseTaskConfig;
pub use models::config::ResolvedConfigSnapshot;
pub use models::config::ThemeMode;
pub use models::config::WorkspaceSettings;
pub use models::config::WorkspaceSettingsPatch;
pub use services::config::ConfigService;
pub use services::config::LGTM_DESKTOP_BIN_ENV;
pub use services::config::LGTM_WORKSPACE_ENV;
