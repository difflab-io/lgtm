use std::path::Path;

use figment::{Figment, providers::Serialized};
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

/// Configuration storage scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    /// User-wide configuration stored in `~/.lgtm/config.{json,yaml,yml}`.
    #[default]
    Global,
    /// Workspace-specific configuration stored in `<workspace>/.lgtm/config.{json,yaml,yml}`.
    Project,
}

/// Serialization format of a configuration file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFormat {
    /// JSON configuration file (`config.json`).
    #[default]
    Json,
    /// YAML configuration file (`config.yaml` or `config.yml`).
    Yaml,
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

/// Sparse nested settings document and patch persisted in `config.json` or `config.yaml` and accepted by `config_update`.
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
    /// Sparse global settings loaded from `~/.lgtm/config.{json,yaml,yml}`.
    pub global: UserSettingsPatch,
    /// Sparse project settings loaded from `<workspace>/.lgtm/config.{json,yaml,yml}`, if present.
    pub project: Option<UserSettingsPatch>,
    /// Absolute path to the active global config file.
    pub global_path: String,
    /// Absolute path to the active project config file.
    pub project_path: String,
}

/// Raw text and LSP metadata for a global or project config file opened in `CodeEditor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDocument {
    /// Scope of this configuration document (`global` or `project`).
    pub scope: ConfigScope,
    /// Serialization format (`json` or `yaml`).
    pub format: ConfigFormat,
    /// Absolute path on disk.
    pub path: String,
    /// Identifier passed as `relativePath` to `lsp_did_change`, `lsp_hover`, and `lsp_goto_definition`.
    pub lsp_path: String,
    /// Tab title rendered in the editor tab bar (for example `Settings (config.json)`).
    pub title: String,
    /// Raw UTF-8 file content.
    pub content: String,
    /// SHA-256 content revision.
    pub revision: String,
    /// Detected language ID (`json` or `yaml`).
    pub language_id: String,
}

impl ConfigFormat {
    /// Detects the `ConfigFormat` from a file path's extension, defaulting to `ConfigFormat::Json`.
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Json,
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns the canonical file name for this format (`config.json` or `config.yaml`).
    pub fn default_file_name(self) -> &'static str {
        match self {
            Self::Json => "config.json",
            Self::Yaml => "config.yaml",
        }
    }

    /// Returns the language identifier (`json` or `yaml`) matching built-in LSP presets.
    pub fn language_id(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }
}

impl UserSettings {
    // Public ------------------------------------------------------------------

    /// Applies a sparse `UserSettingsPatch` onto this resolved settings tree via `figment`.
    pub fn apply_patch(&mut self, patch: &UserSettingsPatch) {
        if let Ok(merged) = Figment::from(Serialized::defaults(&*self))
            .merge(Serialized::defaults(patch))
            .extract::<Self>()
        {
            *self = merged;
        }
    }
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

    /// Builds the default initial config document text for a newly created global or project config file.
    pub fn default_document(format: ConfigFormat) -> String {
        match format {
            ConfigFormat::Json => {
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
            ConfigFormat::Yaml => [
                &format!("# yaml-language-server: $schema={DEFAULT_SCHEMA_URL}"),
                "appearance:",
                "  theme: system",
                "editor:",
                "  lineWrapping: auto",
                "",
            ]
            .join("\n"),
        }
    }

    /// Merges `overlay` onto `self` via `figment`.
    pub fn merge_patch(&mut self, overlay: Self) {
        if let Ok(merged) = Figment::from(Serialized::defaults(&*self))
            .merge(Serialized::defaults(overlay))
            .extract::<Self>()
        {
            *self = merged;
        }
    }

    /// Partitions `self` leaf-by-leaf into `(global_patch, project_patch)` based on which leaves are already overridden in `project_existing`.
    pub fn split_by_project_overrides(
        &self,
        project_existing: Option<&Self>,
    ) -> (Option<Self>, Option<Self>) {
        let Ok(serde_json::Value::Object(incoming_map)) = serde_json::to_value(self) else {
            return (None, None);
        };
        let existing_val = project_existing.and_then(|p| serde_json::to_value(p).ok());
        let existing_map = existing_val.as_ref().and_then(serde_json::Value::as_object);

        let (global_map, project_map) = Self::split_json_maps(&incoming_map, existing_map);
        let global_out = (!global_map.is_empty())
            .then(|| serde_json::from_value(serde_json::Value::Object(global_map)).ok())
            .flatten();
        let project_out = (!project_map.is_empty())
            .then(|| serde_json::from_value(serde_json::Value::Object(project_map)).ok())
            .flatten();
        (global_out, project_out)
    }

    // Private -----------------------------------------------------------------

    fn split_json_maps(
        incoming: &serde_json::Map<String, serde_json::Value>,
        existing: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> (
        serde_json::Map<String, serde_json::Value>,
        serde_json::Map<String, serde_json::Value>,
    ) {
        let mut global = serde_json::Map::new();
        let mut project = serde_json::Map::new();
        for (k, v) in incoming {
            if k == "$schema" || v.is_null() {
                continue;
            }
            let existing_child = existing.and_then(|m| m.get(k));
            if let serde_json::Value::Object(child_map) = v {
                let (g_sub, p_sub) = Self::split_json_maps(
                    child_map,
                    existing_child.and_then(serde_json::Value::as_object),
                );
                if !g_sub.is_empty() {
                    global.insert(k.clone(), serde_json::Value::Object(g_sub));
                }
                if !p_sub.is_empty() {
                    project.insert(k.clone(), serde_json::Value::Object(p_sub));
                }
            } else if existing_child.is_some_and(|ev| !ev.is_null()) {
                project.insert(k.clone(), v.clone());
            } else {
                global.insert(k.clone(), v.clone());
            }
        }
        (global, project)
    }
}
