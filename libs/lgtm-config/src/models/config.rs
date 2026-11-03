use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LgtmConfig {
    pub lsp: LspSettings,
    pub mise: MiseSettings,
    pub languages: BTreeMap<String, LanguageConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LspSettings {
    pub enabled: bool,
    pub auto_install: bool,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MiseSettings {
    pub tools: BTreeMap<String, String>,
    pub tasks: BTreeMap<String, MiseTaskConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MiseTaskConfig {
    Command(String),
    Detailed {
        description: Option<String>,
        run: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageConfig {
    pub enabled: bool,
    pub extensions: Vec<String>,
    pub root_markers: Vec<String>,
    pub mise_tools: BTreeMap<String, String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub initialization_options: Option<serde_json::Value>,
    #[serde(default)]
    pub default_tasks: BTreeMap<String, MiseTaskConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedProject {
    pub language_id: String,
    pub project_root: PathBuf,
    pub root_marker: Option<String>,
}

impl LgtmConfig {
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
                mise_tools: BTreeMap::from([("npm:pyright".to_string(), "latest".to_string())]),
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

        Self {
            lsp: LspSettings::default(),
            mise: MiseSettings::default(),
            languages,
        }
    }

    // Public ------------------------------------------------------------------

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

    pub fn detect_project_root(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        language: &LanguageConfig,
    ) -> (PathBuf, Option<String>) {
        let mut cursor = if file_path.is_dir() {
            file_path.to_path_buf()
        } else {
            file_path.parent().unwrap_or(workspace_root).to_path_buf()
        };

        loop {
            for marker in &language.root_markers {
                if cursor.join(marker).exists() {
                    return (cursor, Some(marker.clone()));
                }
            }
            if cursor == workspace_root {
                break;
            }
            let Some(parent) = cursor.parent() else {
                break;
            };
            if !parent.starts_with(workspace_root) {
                break;
            }
            cursor = parent.to_path_buf();
        }

        (workspace_root.to_path_buf(), None)
    }

    pub fn detect_workspace_languages(&self, workspace_root: &Path) -> Vec<String> {
        let mut detected = BTreeSet::new();

        for (id, cfg) in &self.languages {
            if !cfg.enabled {
                continue;
            }
            if cfg
                .root_markers
                .iter()
                .any(|marker| workspace_root.join(marker).exists())
            {
                detected.insert(id.clone());
            }
        }

        if let Ok(entries) = std::fs::read_dir(workspace_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some((lang_id, _)) = self.detect_file_language(&path) {
                        detected.insert(lang_id);
                    }
                } else if path.is_dir() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with('.')
                        || name_str == "node_modules"
                        || name_str == "target"
                    {
                        continue;
                    }
                    for (id, cfg) in &self.languages {
                        if cfg.enabled
                            && cfg
                                .root_markers
                                .iter()
                                .any(|marker| path.join(marker).exists())
                        {
                            detected.insert(id.clone());
                        }
                    }
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if sub_path.is_file()
                                && let Some((lang_id, _)) = self.detect_file_language(&sub_path)
                            {
                                detected.insert(lang_id);
                            }
                        }
                    }
                }
            }
        }

        detected.into_iter().collect()
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
            auto_install: false,
            request_timeout_ms: 5_000,
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
            fn should_include_rust_typescript_javascript_python_and_go() {
                // Arrange
                let config = LgtmConfig::default();

                // Act
                let keys: Vec<String> = config.languages.keys().cloned().collect();

                // Assert
                assert_eq!(
                    keys,
                    vec![
                        "go".to_string(),
                        "javascript".to_string(),
                        "python".to_string(),
                        "rust".to_string(),
                        "typescript".to_string(),
                    ]
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
                let unknown = config.detect_file_language(Path::new("README.md"));

                // Assert
                assert_eq!(rust.map(|(id, _)| id), Some("rust".to_string()));
                assert_eq!(ts.map(|(id, _)| id), Some("typescript".to_string()));
                assert_eq!(py.map(|(id, _)| id), Some("python".to_string()));
                assert_eq!(go.map(|(id, _)| id), Some("go".to_string()));
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

    mod given_a_workspace_directory {
        use super::*;

        mod when_detecting_project_root {
            use super::*;

            #[test]
            fn should_walk_up_to_nearest_root_marker() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                let crate_root = workspace_root.join("crates/core");
                let file_path = crate_root.join("src/lib.rs");
                std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
                std::fs::write(
                    crate_root.join("Cargo.toml"),
                    "[package]\nname = \"core\"\n",
                )
                .unwrap();
                std::fs::write(&file_path, "pub fn add() {}\n").unwrap();
                let config = LgtmConfig::default();
                let rust_cfg = config.languages.get("rust").unwrap();

                // Act
                let (project_root, marker) =
                    config.detect_project_root(workspace_root, &file_path, rust_cfg);

                // Assert
                assert_eq!(project_root, crate_root);
                assert_eq!(marker, Some("Cargo.toml".to_string()));
            }
        }

        mod when_detecting_workspace_languages {
            use super::*;

            #[test]
            fn should_return_sorted_detected_languages() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                std::fs::write(workspace_root.join("Cargo.toml"), "[workspace]\n").unwrap();
                std::fs::write(workspace_root.join("package.json"), "{}\n").unwrap();
                let config = LgtmConfig::default();

                // Act
                let languages = config.detect_workspace_languages(workspace_root);

                // Assert
                assert_eq!(
                    languages,
                    vec![
                        "javascript".to_string(),
                        "rust".to_string(),
                        "typescript".to_string(),
                    ]
                );
            }
        }
    }
}
