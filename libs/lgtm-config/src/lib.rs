//! Configuration and language discovery for LGTM workspaces.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod models;
mod services;

pub use error::LgtmConfigError;
pub use error::Result;
pub use models::config::LanguageConfig;
pub use models::config::LgtmConfig;
pub use models::config::LspSettings;
pub use models::config::MiseTaskConfig;
pub use models::settings::AppearanceSettings;
pub use models::settings::AppearanceSettingsPatch;
pub use models::settings::ConfigDocument;
pub use models::settings::ConfigFormat;
pub use models::settings::ConfigScope;
pub use models::settings::DEFAULT_SCHEMA_URL;
pub use models::settings::EditorSettings;
pub use models::settings::EditorSettingsPatch;
pub use models::settings::LineWrap;
pub use models::settings::LineWrapPreset;
pub use models::settings::ResolvedConfigSnapshot;
pub use models::settings::ThemeMode;
pub use models::settings::UserSettings;
pub use models::settings::UserSettingsPatch;
pub use models::settings::WorkspaceSettings;
pub use models::settings::WorkspaceSettingsPatch;
pub use services::config::ConfigService;
pub use services::config::LGTM_DESKTOP_BIN_ENV;
pub use services::config::LGTM_WORKSPACE_ENV;
