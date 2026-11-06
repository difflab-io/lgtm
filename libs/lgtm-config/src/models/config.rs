use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::models::settings::{
    AppearanceSettingsPatch, EditorSettingsPatch, WorkspaceSettingsPatch,
};

/// Merged LGTM configuration combining built-in presets, global user settings, and project overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LgtmConfig {
    /// Global LSP lifecycle and timeout settings.
    pub lsp: LspSettings,
    /// User-defined Mise tools and tasks merged into `.lgtm/mise.toml`.
    pub mise: MiseSettings,
    /// Per-language LSP server and Mise toolchain definitions keyed by language ID.
    pub languages: BTreeMap<String, LanguageConfig>,
}

/// Settings controlling LSP server startup, automatic tool installation, and request timeouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LspSettings {
    /// Whether LSP integration is enabled for workspaces.
    pub enabled: bool,
    /// Whether missing language server binaries should be installed via `mise install`.
    pub auto_install: bool,
    /// Maximum duration in milliseconds to wait for an LSP JSON-RPC response.
    pub request_timeout_ms: u64,
}

/// Custom Mise tools and tasks configured in `.lgtm/config.{json,yaml,yml}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MiseSettings {
    /// Tool versions keyed by Mise tool name (for example `ripgrep = "latest"`).
    pub tools: BTreeMap<String, String>,
    /// Named Mise tasks synthesized into `.lgtm/mise.toml`.
    pub tasks: BTreeMap<String, MiseTaskConfig>,
}

/// A task definition in `.lgtm/config.{json,yaml,yml}`, represented as either a shell string or a detailed record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MiseTaskConfig {
    /// Shorthand shell command string.
    Command(String),
    /// Detailed task with an optional description and shell command.
    Detailed {
        /// Optional human-readable task summary.
        description: Option<String>,
        /// Shell command executed by `mise run`.
        run: String,
    },
}

/// Language server and Mise tool preset for a single programming language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub initialization_options: Option<serde_json::Value>,
    /// Default Mise tasks synthesized when this language is detected in a workspace.
    #[serde(default)]
    pub default_tasks: BTreeMap<String, MiseTaskConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LspSettingsOverlay {
    pub(crate) enabled: Option<bool>,
    #[serde(alias = "autoInstall")]
    pub(crate) auto_install: Option<bool>,
    #[serde(alias = "requestTimeoutMs")]
    pub(crate) request_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LgtmConfigFileOverlay {
    #[serde(rename = "$schema")]
    pub(crate) schema: Option<String>,
    pub(crate) appearance: Option<AppearanceSettingsPatch>,
    pub(crate) workspace: Option<WorkspaceSettingsPatch>,
    pub(crate) editor: Option<EditorSettingsPatch>,
    pub(crate) lsp: Option<LspSettingsOverlay>,
    pub(crate) mise: Option<MiseSettings>,
    pub(crate) languages: Option<BTreeMap<String, LanguageConfig>>,
}

impl LgtmConfig {
    /// Builds the default LGTM configuration with presets for Rust, TypeScript, JavaScript, Python, Go, Markdown, JSON, and YAML.
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
                root_markers: vec!["biome.json".to_string(), "biome.jsonc".to_string()],
                mise_tools: BTreeMap::from([("biome".to_string(), "latest".to_string())]),
                command: "biome".to_string(),
                args: vec!["lsp-proxy".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
                default_tasks: BTreeMap::new(),
            },
        );

        languages.insert(
            "yaml".to_string(),
            LanguageConfig {
                enabled: true,
                extensions: vec!["yaml".to_string(), "yml".to_string()],
                root_markers: vec![".yamllint".to_string()],
                mise_tools: BTreeMap::from([
                    ("node".to_string(), "lts".to_string()),
                    ("npm:yaml-language-server".to_string(), "latest".to_string()),
                ]),
                command: "yaml-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                env: BTreeMap::new(),
                initialization_options: None,
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
            lsp: LspSettings::default(),
            mise: MiseSettings::default(),
            languages,
        }
    }

    // Public ------------------------------------------------------------------

    /// Merges an overlay configuration on top of this configuration in place.
    pub fn merge(&mut self, overlay: Self) {
        self.lsp = overlay.lsp;
        for (tool, version) in overlay.mise.tools {
            self.mise.tools.insert(tool, version);
        }
        for (task, config) in overlay.mise.tasks {
            self.mise.tasks.insert(task, config);
        }
        for (language_id, language_config) in overlay.languages {
            self.languages.insert(language_id, language_config);
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

    // Private -----------------------------------------------------------------

    pub(crate) fn merge_file_overlay(&mut self, overlay: LgtmConfigFileOverlay) {
        let _ = (
            overlay.schema,
            overlay.appearance,
            overlay.workspace,
            overlay.editor,
        );
        if let Some(lsp) = overlay.lsp {
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
        if let Some(mise) = overlay.mise {
            for (tool, version) in mise.tools {
                self.mise.tools.insert(tool, version);
            }
            for (task, config) in mise.tasks {
                self.mise.tasks.insert(task, config);
            }
        }
        if let Some(languages) = overlay.languages {
            for (language_id, language_config) in languages {
                self.languages.insert(language_id, language_config);
            }
        }
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
            fn should_include_rust_typescript_javascript_python_go_markdown_json_and_yaml() {
                // Arrange
                let config = LgtmConfig::default();

                // Act
                let keys: Vec<String> = config.languages.keys().cloned().collect();
                let json_preset = config.languages.get("json").unwrap();
                let yaml_preset = config.languages.get("yaml").unwrap();

                // Assert
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
                        "yaml".to_string(),
                    ]
                );
                assert_eq!(json_preset.command, "biome");
                assert_eq!(json_preset.args, vec!["lsp-proxy".to_string()]);
                assert_eq!(
                    json_preset.mise_tools.get("biome"),
                    Some(&"latest".to_string())
                );
                assert_eq!(yaml_preset.command, "yaml-language-server");
                assert_eq!(yaml_preset.args, vec!["--stdio".to_string()]);
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
                let yaml = config.detect_file_language(Path::new(".lgtm/config.yaml"));
                let unknown = config.detect_file_language(Path::new("notes.txt"));

                // Assert
                assert_eq!(rust.map(|(id, _)| id), Some("rust".to_string()));
                assert_eq!(ts.map(|(id, _)| id), Some("typescript".to_string()));
                assert_eq!(py.map(|(id, _)| id), Some("python".to_string()));
                assert_eq!(go.map(|(id, _)| id), Some("go".to_string()));
                assert_eq!(md.map(|(id, _)| id), Some("markdown".to_string()));
                assert_eq!(json.map(|(id, _)| id), Some("json".to_string()));
                assert_eq!(yaml.map(|(id, _)| id), Some("yaml".to_string()));
                assert!(unknown.is_none());
            }
        }
    }

    mod given_global_and_project_configs {
        use super::*;

        mod when_merging_configs {
            use super::*;

            #[test]
            fn should_override_global_entries_with_project_entries() {
                // Arrange
                let mut base = LgtmConfig::default();
                let mut overlay = LgtmConfig::default();
                overlay.lsp.request_timeout_ms = 12_000;
                overlay
                    .mise
                    .tools
                    .insert("ripgrep".to_string(), "latest".to_string());
                overlay.mise.tasks.insert(
                    "custom".to_string(),
                    MiseTaskConfig::Command("echo hi".to_string()),
                );

                // Act
                base.merge(overlay);

                // Assert
                assert_eq!(base.lsp.request_timeout_ms, 12_000);
                assert_eq!(base.mise.tools.get("ripgrep"), Some(&"latest".to_string()));
                assert_eq!(
                    base.mise.tasks.get("custom"),
                    Some(&MiseTaskConfig::Command("echo hi".to_string()))
                );
            }
        }
    }
}
