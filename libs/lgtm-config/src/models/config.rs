use std::collections::BTreeMap;
use std::path::Path;

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

/// Settings controlling LSP server startup, automatic tool installation, and request timeouts (`lsp.*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LspSettings {
    /// Whether LSP integration is enabled for workspaces.
    pub enabled: bool,
    /// Whether missing language server binaries should be installed via `mise install`.
    #[serde(alias = "auto_install")]
    pub auto_install: bool,
    /// Maximum duration in milliseconds to wait for an LSP JSON-RPC response.
    #[serde(alias = "request_timeout_ms")]
    pub request_timeout_ms: u64,
}

/// Sparse LSP settings overrides persisted under `lsp.*` in `config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LspSettingsPatch {
    /// Whether LSP integration is enabled for workspaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Whether missing language server binaries should be installed via `mise install`.
    #[serde(alias = "auto_install", skip_serializing_if = "Option::is_none")]
    pub auto_install: Option<bool>,
    /// Maximum duration in milliseconds to wait for an LSP JSON-RPC response.
    #[serde(alias = "request_timeout_ms", skip_serializing_if = "Option::is_none")]
    pub request_timeout_ms: Option<u64>,
}

/// A default Mise task definition for a language preset, represented as either a shell string or a detailed record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(untagged)]
pub enum MiseTaskConfig {
    /// Shorthand shell command string.
    Command(String),
    /// Detailed task with an optional description and shell command.
    Detailed {
        /// Optional human-readable task summary.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Shell command executed by `mise run`.
        run: String,
    },
}

/// Language server and Mise tool preset for a single programming language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
pub struct LanguageConfig {
    /// Whether this language preset is active.
    pub enabled: bool,
    /// File extensions (without leading dots) mapped to this language.
    pub extensions: Vec<String>,
    /// Marker files or directories that identify the language's project root.
    pub root_markers: Vec<String>,
    /// Mise tool specifications required to run this language server.
    pub mise_tools: BTreeMap<String, String>,
    /// Executable name or path for the language server.
    pub command: String,
    /// Command-line arguments passed to the language server executable.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables passed to the language server process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional JSON payload forwarded as `initializationOptions` during the LSP `initialize` handshake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(skip)]
    pub initialization_options: Option<serde_json::Value>,
    /// Default Mise tasks synthesized when this language is detected in a workspace.
    #[serde(default)]
    pub default_tasks: BTreeMap<String, MiseTaskConfig>,
}

/// Complete resolved LGTM configuration combining built-in presets, global `~/.lgtm/config.json`, and project `<workspace>/.lgtm/config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LgtmConfig {
    /// Optional JSON Schema reference URI for editor intelligence.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Appearance and theme settings (`appearance.*`).
    pub appearance: AppearanceSettings,
    /// Workspace history and active folder settings (`workspace.*`).
    pub workspace: WorkspaceSettings,
    /// Code editor display settings (`editor.*`).
    pub editor: EditorSettings,
    /// Global LSP lifecycle and timeout settings (`lsp.*`).
    pub lsp: LspSettings,
    /// Per-language LSP server and Mise toolchain definitions keyed by language ID (`languages.*`).
    pub languages: BTreeMap<String, LanguageConfig>,
}

/// Sparse nested configuration document persisted in `~/.lgtm/config.json` and `<workspace>/.lgtm/config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LgtmConfigPatch {
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
    /// Partial LSP lifecycle and timeout overrides (`lsp.*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp: Option<LspSettingsPatch>,
    /// Per-language LSP server and Mise toolchain overrides keyed by language ID (`languages.*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<BTreeMap<String, LanguageConfig>>,
}

/// Merged configuration snapshot together with per-scope patch trees returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfigSnapshot {
    /// Effective deep-merged configuration (project leaf overrides layered over global settings and defaults).
    pub effective: LgtmConfig,
    /// Sparse global configuration loaded from `~/.lgtm/config.json`.
    pub global: LgtmConfigPatch,
    /// Sparse project configuration loaded from `<workspace>/.lgtm/config.json`, if present.
    pub project: Option<LgtmConfigPatch>,
    /// Absolute path to the active global config file.
    pub global_path: String,
    /// Absolute path to the active project config file.
    pub project_path: String,
}

impl LgtmConfig {
    /// Generates the JSON Schema value for `LgtmConfig` (`schemas/config.schema.json`).
    pub fn json_schema() -> serde_json::Value {
        let schema = schemars::schema_for!(LgtmConfig);
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

    /// Builds the default LGTM configuration with presets for Rust, TypeScript, JavaScript, Python, Go, Markdown, and JSON.
    pub fn default_presets() -> Self {
        let mut languages = BTreeMap::new();

        languages.insert(
            "rust".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["rs".to_string()],
                root_markers: vec!["Cargo.toml".to_string()],
                mise_tools: BTreeMap::from([("rust-analyzer".to_string(), "latest".to_string())]),
                command: "rust-analyzer".to_string(),
                args: Vec::new(),
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::from([
                    (
                        "check".to_string(),
                        MiseTaskConfig::Detailed {
                            description: Some("Typecheck Rust workspace".to_string()),
                            run: "cargo check".to_string(),
                        },
                    ),
                    (
                        "test".to_string(),
                        MiseTaskConfig::Detailed {
                            description: Some("Run Rust tests".to_string()),
                            run: "cargo test".to_string(),
                        },
                    ),
                ]),
            },
        );

        let ts_tasks = BTreeMap::from([(
            "check".to_string(),
            MiseTaskConfig::Detailed {
                description: Some("Typecheck TypeScript project".to_string()),
                run: "tsc --noEmit".to_string(),
            },
        )]);

        languages.insert(
            "typescript".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec![
                    "ts".to_string(),
                    "tsx".to_string(),
                    "mts".to_string(),
                    "cts".to_string(),
                ],
                root_markers: vec!["tsconfig.json".to_string(), "package.json".to_string()],
                mise_tools: BTreeMap::from([
                    ("node".to_string(), "lts".to_string()),
                    (
                        "npm:typescript-language-server".to_string(),
                        "latest".to_string(),
                    ),
                    ("npm:typescript".to_string(), "latest".to_string()),
                ]),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: ts_tasks,
            },
        );

        languages.insert(
            "javascript".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec![
                    "js".to_string(),
                    "jsx".to_string(),
                    "mjs".to_string(),
                    "cjs".to_string(),
                ],
                root_markers: vec!["package.json".to_string(), "jsconfig.json".to_string()],
                mise_tools: BTreeMap::from([
                    ("node".to_string(), "lts".to_string()),
                    (
                        "npm:typescript-language-server".to_string(),
                        "latest".to_string(),
                    ),
                    ("npm:typescript".to_string(), "latest".to_string()),
                ]),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::new(),
            },
        );

        languages.insert(
            "python".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["py".to_string(), "pyi".to_string()],
                root_markers: vec!["pyproject.toml".to_string(), "requirements.txt".to_string()],
                mise_tools: BTreeMap::from([
                    ("node".to_string(), "lts".to_string()),
                    ("npm:pyright".to_string(), "latest".to_string()),
                ]),
                command: "pyright-langserver".to_string(),
                args: vec!["--stdio".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::from([(
                    "check".to_string(),
                    MiseTaskConfig::Detailed {
                        description: Some("Typecheck Python project with Pyright".to_string()),
                        run: "pyright".to_string(),
                    },
                )]),
            },
        );

        languages.insert(
            "go".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["go".to_string()],
                root_markers: vec!["go.mod".to_string()],
                mise_tools: BTreeMap::from([("gopls".to_string(), "latest".to_string())]),
                command: "gopls".to_string(),
                args: Vec::new(),
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::from([
                    (
                        "check".to_string(),
                        MiseTaskConfig::Detailed {
                            description: Some("Vet Go packages".to_string()),
                            run: "go vet ./...".to_string(),
                        },
                    ),
                    (
                        "test".to_string(),
                        MiseTaskConfig::Detailed {
                            description: Some("Run Go tests".to_string()),
                            run: "go test ./...".to_string(),
                        },
                    ),
                ]),
            },
        );

        languages.insert(
            "markdown".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["md".to_string(), "markdown".to_string(), "mdx".to_string()],
                root_markers: vec![".marksman.toml".to_string()],
                mise_tools: BTreeMap::from([("marksman".to_string(), "latest".to_string())]),
                command: "marksman".to_string(),
                args: vec!["server".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::new(),
            },
        );

        languages.insert(
            "json".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["json".to_string(), "jsonc".to_string()],
                root_markers: Vec::new(),
                mise_tools: BTreeMap::from([
                    ("node".to_string(), "22".to_string()),
                    (
                        "npm:vscode-langservers-extracted".to_string(),
                        "latest".to_string(),
                    ),
                ]),
                command: "vscode-json-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                env: BTreeMap::new(),
                initialization_options: Some(serde_json::json!({
                    "provideFormatter": true
                })),
                default_tasks: BTreeMap::new(),
            },
        );

        if let Some(script) = std::env::var_os("LGTM_E2E_LSP_SCRIPT")
            .map(|val| val.to_string_lossy().trim().to_string())
            .filter(|val| !val.is_empty())
        {
            for lang in languages.values_mut() {
                lang.command = "node".to_string();
                lang.args = vec![script.clone()];
                lang.mise_tools.clear();
            }
        }

        Self {
            schema: None,
            appearance: AppearanceSettings::default(),
            workspace: WorkspaceSettings::default(),
            editor: EditorSettings::default(),
            lsp: LspSettings::default(),
            languages,
        }
    }

    // Public ------------------------------------------------------------------

    /// Applies a sparse `LgtmConfigPatch` onto this resolved configuration.
    pub fn apply_patch(&mut self, patch: &LgtmConfigPatch) {
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
        if let Some(lsp) = &patch.lsp {
            if let Some(enabled) = lsp.enabled {
                self.lsp.enabled = enabled;
            }
            if let Some(auto_install) = lsp.auto_install {
                self.lsp.auto_install = auto_install;
            }
            if let Some(timeout) = lsp.request_timeout_ms {
                self.lsp.request_timeout_ms = timeout;
            }
        }
        if let Some(languages) = &patch.languages {
            for (language_id, language_config) in languages {
                self.languages
                    .insert(language_id.clone(), language_config.clone());
            }
        }
    }

    /// Resolves the enabled language ID and configuration matching a file's extension.
    pub fn detect_file_language(&self, file_path: &Path) -> Option<(String, &LanguageConfig)> {
        let ext = file_path.extension()?.to_str()?.to_ascii_lowercase();
        self.languages.iter().find_map(|(id, cfg)| {
            if cfg.enabled && cfg.extensions.iter().any(|candidate| candidate == &ext) {
                Some((id.clone(), cfg))
            } else {
                None
            }
        })
    }
}

impl Default for LgtmConfig {
    fn default() -> Self {
        Self::default_presets()
    }
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_install: true,
            request_timeout_ms: 15_000,
        }
    }
}

impl LgtmConfigPatch {
    // Public ------------------------------------------------------------------

    /// Builds the default initial `config.json` document text for a newly created global config file.
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
            lsp: None,
            languages: None,
        };
        let mut text =
            serde_json::to_string_pretty(&initial).unwrap_or_else(|_| "{\n}\n".to_string());
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }

    /// Parses a `config.json` string leniently, extracting valid fields and ignoring unknown properties, invalid leaf values, or syntax errors.
    pub fn from_str_lenient(raw: &str) -> Self {
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(raw)
        else {
            return Self::default();
        };

        let schema = root
            .get("$schema")
            .and_then(|v| serde_json::from_value::<String>(v.clone()).ok());

        let appearance = root
            .get("appearance")
            .and_then(serde_json::Value::as_object)
            .map(|obj| AppearanceSettingsPatch {
                theme: obj
                    .get("theme")
                    .and_then(|v| serde_json::from_value::<ThemeMode>(v.clone()).ok()),
            });

        let workspace = root
            .get("workspace")
            .and_then(serde_json::Value::as_object)
            .map(|obj| WorkspaceSettingsPatch {
                active_root: obj
                    .get("activeRoot")
                    .and_then(|v| serde_json::from_value::<String>(v.clone()).ok()),
                recent_roots: obj
                    .get("recentRoots")
                    .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()),
            });

        let editor = root
            .get("editor")
            .and_then(serde_json::Value::as_object)
            .map(|obj| EditorSettingsPatch {
                line_wrapping: obj
                    .get("lineWrapping")
                    .and_then(|v| serde_json::from_value::<LineWrap>(v.clone()).ok()),
            });

        let lsp = root
            .get("lsp")
            .and_then(serde_json::Value::as_object)
            .map(|obj| LspSettingsPatch {
                enabled: obj
                    .get("enabled")
                    .and_then(|v| serde_json::from_value::<bool>(v.clone()).ok()),
                auto_install: obj
                    .get("autoInstall")
                    .or_else(|| obj.get("auto_install"))
                    .and_then(|v| serde_json::from_value::<bool>(v.clone()).ok()),
                request_timeout_ms: obj
                    .get("requestTimeoutMs")
                    .or_else(|| obj.get("request_timeout_ms"))
                    .and_then(|v| serde_json::from_value::<u64>(v.clone()).ok()),
            });

        let languages = root
            .get("languages")
            .and_then(serde_json::Value::as_object)
            .map(|obj| {
                obj.iter()
                    .filter_map(|(key, val)| {
                        serde_json::from_value::<LanguageConfig>(val.clone())
                            .ok()
                            .map(|cfg| (key.clone(), cfg))
                    })
                    .collect::<BTreeMap<String, LanguageConfig>>()
            })
            .filter(|map| !map.is_empty());

        Self {
            schema,
            appearance,
            workspace,
            editor,
            lsp,
            languages,
        }
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_default_lgtm_config {
        use super::*;

        mod when_resolving_built_in_language_presets {
            use super::*;

            #[test]
            fn should_include_rust_typescript_javascript_python_go_markdown_and_json() {
                // Arrange
                let config = LgtmConfig::default();

                // Act
                let keys: Vec<String> = config.languages.keys().cloned().collect();
                let json_preset = config.languages.get("json").unwrap();

                // Assert
                assert_eq!(config.appearance.theme, ThemeMode::System);
                assert_eq!(config.editor.line_wrapping, LineWrap::Auto);
                assert_eq!(
                    keys,
                    vec![
                        "go".to_string(),
                        "javascript".to_string(),
                        "json".to_string(),
                        "markdown".to_string(),
                        "python".to_string(),
                        "rust".to_string(),
                        "typescript".to_string(),
                    ]
                );
                assert_eq!(json_preset.command, "vscode-json-language-server");
                assert_eq!(json_preset.args, vec!["--stdio".to_string()]);
                assert_eq!(
                    json_preset
                        .mise_tools
                        .get("npm:vscode-langservers-extracted"),
                    Some(&"latest".to_string())
                );
            }
        }

        mod when_detecting_file_language {
            use super::*;

            #[test]
            fn should_resolve_language_from_extension() {
                // Arrange
                let config = LgtmConfig::default();

                // Act
                let rust = config.detect_file_language(Path::new("src/main.rs"));
                let ts = config.detect_file_language(Path::new("web/app.ts"));
                let py = config.detect_file_language(Path::new("scripts/run.py"));
                let go = config.detect_file_language(Path::new("cmd/main.go"));
                let md = config.detect_file_language(Path::new("README.md"));
                let json = config.detect_file_language(Path::new(".lgtm/config.json"));
                let unknown = config.detect_file_language(Path::new("notes.txt"));

                // Assert
                assert_eq!(rust.map(|(id, _)| id), Some("rust".to_string()));
                assert_eq!(ts.map(|(id, _)| id), Some("typescript".to_string()));
                assert_eq!(py.map(|(id, _)| id), Some("python".to_string()));
                assert_eq!(go.map(|(id, _)| id), Some("go".to_string()));
                assert_eq!(md.map(|(id, _)| id), Some("markdown".to_string()));
                assert_eq!(json.map(|(id, _)| id), Some("json".to_string()));
                assert!(unknown.is_none());
            }
        }
    }
}
