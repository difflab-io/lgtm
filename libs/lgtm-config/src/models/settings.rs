use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Relative path from `.lgtm/` to the generated JSON Schema file.
pub const DEFAULT_SCHEMA_RELATIVE_PATH: &str = "./schemas/config.schema.json";

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AppearanceSettings {
    /// Preferred application color theme (`system`, `light`, or `dark`).
    pub theme: ThemeMode,
}

/// Partial patch for the `appearance` settings section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AppearanceSettingsPatch {
    /// Updated color theme preference, if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeMode>,
}

/// Resolved workspace history settings section (`workspace.*`).
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

/// Partial patch for the `workspace` settings section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WorkspaceSettingsPatch {
    /// Updated active workspace root path, if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_root: Option<String>,
    /// Updated list of recent workspace root paths, if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_roots: Option<Vec<String>>,
}

/// Resolved code editor settings section (`editor.*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct EditorSettings {
    /// Whether long lines wrap inside the code editor (`true` by default).
    pub line_wrapping: bool,
}

/// Partial patch for the `editor` settings section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct EditorSettingsPatch {
    /// Updated line wrapping preference, if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_wrapping: Option<bool>,
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

/// A schema or syntax validation issue located within a `config.{json,yaml,yml}` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationIssue {
    /// Zero-based line number of the issue.
    pub line: u32,
    /// Zero-based character column on `line`.
    pub character: u32,
    /// Zero-based end character column on `line`.
    pub end_character: u32,
    /// Human-readable validation message.
    pub message: String,
    /// Diagnostic code identifier.
    pub code: String,
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

impl AppearanceSettingsPatch {
    // Public ------------------------------------------------------------------

    /// Returns `true` if no fields in the `appearance` section are set.
    pub fn is_empty(&self) -> bool {
        self.theme.is_none()
    }

    /// Merges `overlay` leaf fields on top of `self`.
    pub fn merge(&mut self, overlay: Self) {
        if let Some(theme) = overlay.theme {
            self.theme = Some(theme);
        }
    }
}

impl WorkspaceSettingsPatch {
    // Public ------------------------------------------------------------------

    /// Returns `true` if no fields in the `workspace` section are set.
    pub fn is_empty(&self) -> bool {
        self.active_root.is_none() && self.recent_roots.is_none()
    }

    /// Merges `overlay` leaf fields on top of `self`.
    pub fn merge(&mut self, overlay: Self) {
        if let Some(active_root) = overlay.active_root {
            self.active_root = Some(active_root);
        }
        if let Some(recent_roots) = overlay.recent_roots {
            self.recent_roots = Some(recent_roots);
        }
    }
}

impl EditorSettingsPatch {
    // Public ------------------------------------------------------------------

    /// Returns `true` if no fields in the `editor` section are set.
    pub fn is_empty(&self) -> bool {
        self.line_wrapping.is_none()
    }

    /// Merges `overlay` leaf fields on top of `self`.
    pub fn merge(&mut self, overlay: Self) {
        if let Some(line_wrapping) = overlay.line_wrapping {
            self.line_wrapping = Some(line_wrapping);
        }
    }
}

impl UserSettings {
    // Public ------------------------------------------------------------------

    /// Applies a sparse `UserSettingsPatch` recursively at the leaf level onto this resolved settings tree.
    pub fn apply_patch(&mut self, patch: &UserSettingsPatch) {
        if let Some(schema) = &patch.schema {
            self.schema = Some(schema.clone());
        }
        if let Some(appearance) = &patch.appearance
            && let Some(theme) = appearance.theme
        {
            self.appearance.theme = theme;
        }
        if let Some(workspace) = &patch.workspace {
            if let Some(active_root) = &workspace.active_root {
                self.workspace.active_root = Some(active_root.clone());
            }
            if let Some(recent_roots) = &workspace.recent_roots {
                self.workspace.recent_roots = recent_roots.clone();
            }
        }
        if let Some(editor) = &patch.editor
            && let Some(line_wrapping) = editor.line_wrapping
        {
            self.editor.line_wrapping = line_wrapping;
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
                    schema: Some(DEFAULT_SCHEMA_RELATIVE_PATH.to_string()),
                    appearance: Some(AppearanceSettingsPatch {
                        theme: Some(ThemeMode::System),
                    }),
                    workspace: None,
                    editor: Some(EditorSettingsPatch {
                        line_wrapping: Some(true),
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
                &format!("# yaml-language-server: $schema={DEFAULT_SCHEMA_RELATIVE_PATH}"),
                "appearance:",
                "  theme: system",
                "editor:",
                "  lineWrapping: true",
                "",
            ]
            .join("\n"),
        }
    }

    /// Returns `true` if no nested settings leaf is set in this patch.
    pub fn is_empty(&self) -> bool {
        self.appearance
            .as_ref()
            .is_none_or(AppearanceSettingsPatch::is_empty)
            && self
                .workspace
                .as_ref()
                .is_none_or(WorkspaceSettingsPatch::is_empty)
            && self
                .editor
                .as_ref()
                .is_none_or(EditorSettingsPatch::is_empty)
    }

    /// Recursively merges `overlay` leaf fields into `self` and prunes empty nested sections.
    pub fn merge_patch(&mut self, overlay: Self) {
        if let Some(schema) = overlay.schema {
            self.schema = Some(schema);
        }
        if let Some(overlay_appearance) = overlay.appearance {
            let entry = self.appearance.get_or_insert_with(Default::default);
            entry.merge(overlay_appearance);
            if entry.is_empty() {
                self.appearance = None;
            }
        }
        if let Some(overlay_workspace) = overlay.workspace {
            let entry = self.workspace.get_or_insert_with(Default::default);
            entry.merge(overlay_workspace);
            if entry.is_empty() {
                self.workspace = None;
            }
        }
        if let Some(overlay_editor) = overlay.editor {
            let entry = self.editor.get_or_insert_with(Default::default);
            entry.merge(overlay_editor);
            if entry.is_empty() {
                self.editor = None;
            }
        }
    }

    /// Partitions `self` leaf-by-leaf into `(global_patch, project_patch)` based on which leaves are already overridden in `project_existing`.
    pub fn split_by_project_overrides(
        &self,
        project_existing: Option<&Self>,
    ) -> (Option<Self>, Option<Self>) {
        let mut global = Self::default();
        let mut project = Self::default();

        if let Some(appearance) = &self.appearance
            && let Some(theme) = appearance.theme
        {
            let overridden_in_project = project_existing
                .and_then(|p| p.appearance.as_ref())
                .and_then(|a| a.theme)
                .is_some();
            let target = if overridden_in_project {
                &mut project
            } else {
                &mut global
            };
            target.appearance.get_or_insert_with(Default::default).theme = Some(theme);
        }

        if let Some(workspace) = &self.workspace {
            if let Some(active_root) = &workspace.active_root {
                let overridden_in_project = project_existing
                    .and_then(|p| p.workspace.as_ref())
                    .and_then(|w| w.active_root.as_ref())
                    .is_some();
                let target = if overridden_in_project {
                    &mut project
                } else {
                    &mut global
                };
                target
                    .workspace
                    .get_or_insert_with(Default::default)
                    .active_root = Some(active_root.clone());
            }
            if let Some(recent_roots) = &workspace.recent_roots {
                let overridden_in_project = project_existing
                    .and_then(|p| p.workspace.as_ref())
                    .and_then(|w| w.recent_roots.as_ref())
                    .is_some();
                let target = if overridden_in_project {
                    &mut project
                } else {
                    &mut global
                };
                target
                    .workspace
                    .get_or_insert_with(Default::default)
                    .recent_roots = Some(recent_roots.clone());
            }
        }

        if let Some(editor) = &self.editor
            && let Some(line_wrapping) = editor.line_wrapping
        {
            let overridden_in_project = project_existing
                .and_then(|p| p.editor.as_ref())
                .and_then(|e| e.line_wrapping)
                .is_some();
            let target = if overridden_in_project {
                &mut project
            } else {
                &mut global
            };
            target
                .editor
                .get_or_insert_with(Default::default)
                .line_wrapping = Some(line_wrapping);
        }

        let global_out = (!global.is_empty()).then_some(global);
        let project_out = (!project.is_empty()).then_some(project);
        (global_out, project_out)
    }

    /// Validates raw JSON or YAML configuration text against `UserSettingsPatch` and returns any syntax or schema issues.
    pub fn validate_document_text(format: ConfigFormat, text: &str) -> Vec<ConfigValidationIssue> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        match format {
            ConfigFormat::Json => match serde_json::from_str::<Self>(text) {
                Ok(_) => Vec::new(),
                Err(err) => {
                    let line = err.line().saturating_sub(1) as u32;
                    let col = err.column().saturating_sub(1) as u32;
                    let (adj_line, start_col, end_col) =
                        Self::locate_issue_span(text, line, col, &err.to_string());
                    vec![ConfigValidationIssue {
                        line: adj_line,
                        character: start_col,
                        end_character: end_col,
                        message: err.to_string(),
                        code: "config-schema".to_string(),
                    }]
                }
            },
            ConfigFormat::Yaml => match serde_yaml::from_str::<Self>(text) {
                Ok(_) => Vec::new(),
                Err(err) => {
                    let (line, col) = err
                        .location()
                        .map(|loc| {
                            (
                                loc.line().saturating_sub(1) as u32,
                                loc.column().saturating_sub(1) as u32,
                            )
                        })
                        .unwrap_or((0, 0));
                    let (adj_line, start_col, end_col) =
                        Self::locate_issue_span(text, line, col, &err.to_string());
                    vec![ConfigValidationIssue {
                        line: adj_line,
                        character: start_col,
                        end_character: end_col,
                        message: err.to_string(),
                        code: "config-schema".to_string(),
                    }]
                }
            },
        }
    }

    /// Returns Markdown hover documentation for a configuration key if `symbol` matches a known schema property.
    pub fn hover_documentation_for_key(symbol: &str) -> Option<&'static str> {
        match symbol {
            "$schema" => Some(
                "**`$schema`** (`string`)\n\nJSON Schema URI used to validate LGTM configuration documents (`./schemas/config.schema.json`).",
            ),
            "appearance" => Some(
                "**`appearance`** (`AppearanceSettings`)\n\nAppearance and color scheme preferences (`appearance.theme`).",
            ),
            "theme" => Some(
                "**`appearance.theme`** (`\"system\" | \"light\" | \"dark\"`, default: `\"system\"`)\n\nPreferred application color theme.",
            ),
            "workspace" => Some(
                "**`workspace`** (`WorkspaceSettings`)\n\nWorkspace history and active directory settings (`workspace.activeRoot`, `workspace.recentRoots`).",
            ),
            "activeRoot" => Some(
                "**`workspace.activeRoot`** (`string | null`)\n\nLast active workspace root directory path.",
            ),
            "recentRoots" => Some(
                "**`workspace.recentRoots`** (`string[]`)\n\nOrdered list of recently opened workspace root directory paths.",
            ),
            "editor" => Some(
                "**`editor`** (`EditorSettings`)\n\nCode editor display preferences (`editor.lineWrapping`).",
            ),
            "lineWrapping" => Some(
                "**`editor.lineWrapping`** (`boolean`, default: `true`)\n\nWhether long lines wrap inside the code editor.",
            ),
            _ => None,
        }
    }

    // Private -----------------------------------------------------------------

    fn locate_issue_span(
        text: &str,
        fallback_line: u32,
        fallback_col: u32,
        error_msg: &str,
    ) -> (u32, u32, u32) {
        if let Some(token) = Self::extract_backtick_token(error_msg) {
            for (idx, line_str) in text.lines().enumerate() {
                if let Some(byte_pos) = line_str.find(&token) {
                    let start_col = line_str[..byte_pos].encode_utf16().count() as u32;
                    let end_col = start_col + (token.encode_utf16().count() as u32).max(1);
                    return (idx as u32, start_col, end_col);
                }
            }
        }
        let line_str = text.lines().nth(fallback_line as usize).unwrap_or("");
        let line_len = line_str.encode_utf16().count() as u32;
        let start = fallback_col.min(line_len.saturating_sub(1));
        let end = (start + 4).min(line_len).max(start + 1);
        (fallback_line, start, end)
    }

    fn extract_backtick_token(error_msg: &str) -> Option<String> {
        let start = error_msg.find('`')?;
        let rest = &error_msg[start + 1..];
        let end = rest.find('`')?;
        let token = &rest[..end];
        (!token.is_empty()).then(|| token.to_string())
    }
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            line_wrapping: true,
        }
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_nested_user_settings_and_patches {
        use super::*;

        mod when_applying_sparse_nested_patches {
            use super::*;

            #[test]
            fn should_deep_merge_leaf_fields_without_resetting_sibling_sections() {
                // Arrange
                let mut settings = UserSettings::default();
                let global_patch = UserSettingsPatch {
                    schema: None,
                    appearance: Some(AppearanceSettingsPatch {
                        theme: Some(ThemeMode::Dark),
                    }),
                    workspace: Some(WorkspaceSettingsPatch {
                        active_root: Some("/tmp/ws-a".to_string()),
                        recent_roots: Some(vec!["/tmp/ws-a".to_string()]),
                    }),
                    editor: Some(EditorSettingsPatch {
                        line_wrapping: Some(true),
                    }),
                };
                let project_patch = UserSettingsPatch {
                    schema: None,
                    appearance: Some(AppearanceSettingsPatch {
                        theme: Some(ThemeMode::Light),
                    }),
                    workspace: None,
                    editor: None,
                };

                // Act
                settings.apply_patch(&global_patch);
                settings.apply_patch(&project_patch);

                // Assert
                assert_eq!(settings.appearance.theme, ThemeMode::Light);
                assert_eq!(
                    settings.workspace.active_root,
                    Some("/tmp/ws-a".to_string())
                );
                assert_eq!(
                    settings.workspace.recent_roots,
                    vec!["/tmp/ws-a".to_string()]
                );
                assert!(settings.editor.line_wrapping);
            }
        }

        mod when_splitting_patch_by_project_overrides {
            use super::*;

            #[test]
            fn should_route_overridden_leaves_to_project_and_remaining_leaves_to_global() {
                // Arrange
                let project_existing = UserSettingsPatch {
                    schema: None,
                    appearance: Some(AppearanceSettingsPatch {
                        theme: Some(ThemeMode::Light),
                    }),
                    workspace: None,
                    editor: None,
                };
                let incoming = UserSettingsPatch {
                    schema: None,
                    appearance: Some(AppearanceSettingsPatch {
                        theme: Some(ThemeMode::Dark),
                    }),
                    workspace: Some(WorkspaceSettingsPatch {
                        active_root: Some("/tmp/ws-b".to_string()),
                        recent_roots: None,
                    }),
                    editor: Some(EditorSettingsPatch {
                        line_wrapping: Some(false),
                    }),
                };

                // Act
                let (global_part, project_part) =
                    incoming.split_by_project_overrides(Some(&project_existing));

                // Assert
                let global = global_part.expect("expected global patch");
                let project = project_part.expect("expected project patch");
                assert!(global.appearance.is_none());
                assert_eq!(
                    global.workspace.and_then(|w| w.active_root),
                    Some("/tmp/ws-b".to_string())
                );
                assert_eq!(global.editor.and_then(|e| e.line_wrapping), Some(false));
                assert_eq!(
                    project.appearance.and_then(|a| a.theme),
                    Some(ThemeMode::Dark)
                );
                assert!(project.workspace.is_none());
                assert!(project.editor.is_none());
            }
        }

        mod when_validating_json_and_yaml_documents {
            use super::*;

            #[test]
            fn should_report_invalid_enum_values_and_unknown_nested_fields() {
                // Arrange
                let invalid_json = "{\n  \"appearance\": {\n    \"theme\": \"neon\"\n  }\n}\n";
                let invalid_yaml = "appearance:\n  unknownNested: true\n";

                // Act
                let json_issues =
                    UserSettingsPatch::validate_document_text(ConfigFormat::Json, invalid_json);
                let yaml_issues =
                    UserSettingsPatch::validate_document_text(ConfigFormat::Yaml, invalid_yaml);

                // Assert
                assert_eq!(json_issues.len(), 1);
                assert_eq!(json_issues[0].line, 2);
                assert!(json_issues[0].message.contains("neon"));
                assert_eq!(yaml_issues.len(), 1);
                assert_eq!(yaml_issues[0].line, 1);
                assert!(yaml_issues[0].message.contains("unknownNested"));
            }
        }
    }
}
