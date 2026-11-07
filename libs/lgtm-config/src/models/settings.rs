use optfield::optfield;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Versioned jsDelivr URL for the repository-tracked `schemas/config.schema.json`.
pub const DEFAULT_SCHEMA_URL: &str = concat!(
    "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v",
    env!("CARGO_PKG_VERSION"),
    "/schemas/config.schema.json",
);

/// Line wrapping mode persisted under `editor.lineWrapping` (`"off"`, `"auto"`, or a column width).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineWrap {
    /// Disable line wrapping (`off`).
    Off,
    /// Wrap lines automatically at the editor viewport width (`auto`).
    #[default]
    Auto,
    /// Fixed character column width at which lines wrap.
    #[serde(untagged)]
    Column(u32),
}

/// Supported color theme preferences persisted under `appearance.theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the operating system color scheme.
    #[default]
    System,
    /// Force the light color scheme.
    Light,
    /// Force the dark color scheme.
    Dark,
}

/// Resolved appearance settings section (`appearance.*`).
#[optfield(
    pub AppearanceSettingsPatch,
    doc,
    attrs,
    field_doc,
    field_attrs = (serde(skip_serializing_if = "Option::is_none")),
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AppearanceSettings {
    /// Preferred application color theme (`system`, `light`, or `dark`).
    pub theme: ThemeMode,
}

/// Resolved workspace history settings section (`workspace.*`).
#[optfield(
    pub WorkspaceSettingsPatch,
    doc,
    attrs,
    field_doc,
    field_attrs = (serde(skip_serializing_if = "Option::is_none")),
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WorkspaceSettings {
    /// Last active workspace root directory path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_root: Option<String>,
    /// Recently opened workspace root directory paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_roots: Vec<String>,
}

/// Resolved code editor settings section (`editor.*`).
#[optfield(
    pub EditorSettingsPatch,
    doc,
    attrs,
    field_doc,
    field_attrs = (serde(skip_serializing_if = "Option::is_none")),
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct EditorSettings {
    /// Line wrapping mode inside the code editor (`"off"`, `"auto"`, or a column width; `"auto"` by default).
    pub line_wrapping: LineWrap,
}

/// Complete resolved nested settings tree combining defaults, global settings, and project overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct UserSettings {
    /// Optional JSON Schema reference URI for editor intelligence.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Appearance and theme settings (`appearance.*`).
    pub appearance: AppearanceSettings,
    /// Workspace history and active folder settings (`workspace.*`).
    pub workspace: WorkspaceSettings,
    /// Code editor display settings (`editor.*`).
    pub editor: EditorSettings,
}

/// Sparse nested settings document and patch persisted in `config.json` and accepted by `config_update`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct UserSettingsPatch {
    /// Optional JSON Schema reference URI for editor intelligence.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Partial appearance settings overrides (`appearance.*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<AppearanceSettingsPatch>,
    /// Partial workspace settings overrides (`workspace.*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceSettingsPatch>,
    /// Partial code editor settings overrides (`editor.*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<EditorSettingsPatch>,
}

/// Merged nested settings snapshot together with per-scope patch trees returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfigSnapshot {
    /// Effective deep-merged settings (project leaf overrides layered over global settings and defaults).
    pub effective: UserSettings,
    /// Sparse global settings loaded from `~/.lgtm/config.json`.
    pub global: UserSettingsPatch,
    /// Sparse project settings loaded from `<workspace>/.lgtm/config.json`, if present.
    pub project: Option<UserSettingsPatch>,
    /// Absolute path to the active global config file.
    pub global_path: String,
    /// Absolute path to the active project config file.
    pub project_path: String,
}

impl UserSettingsPatch {
    // Public ------------------------------------------------------------------

    /// Generates the JSON Schema value for `UserSettingsPatch`.
    pub fn json_schema() -> serde_json::Value {
        let schema = schemars::schema_for!(UserSettingsPatch);
        serde_json::to_value(&schema).unwrap_or_else(|_| serde_json::json!({}))
    }

    /// Generates the pretty-printed JSON Schema string for `schemas/config.schema.json`.
    pub fn json_schema_pretty() -> String {
        let mut s = serde_json::to_string_pretty(&Self::json_schema())
            .unwrap_or_else(|_| "{\n}\n".to_string());
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    }

    /// Builds the default initial `config.json` document text for a newly created global or project config file.
    pub fn default_document() -> String {
        let initial = Self {
            schema: Some(DEFAULT_SCHEMA_URL.to_string()),
            appearance: Some(AppearanceSettingsPatch {
                theme: Some(ThemeMode::System),
            }),
            workspace: None,
            editor: Some(EditorSettingsPatch {
                line_wrapping: Some(LineWrap::default()),
            }),
        };
        let mut text =
            serde_json::to_string_pretty(&initial).unwrap_or_else(|_| "{\n}\n".to_string());
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }
}
