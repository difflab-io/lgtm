use std::fs;
use std::path::{Path, PathBuf};

use lgtm_config::{
    AppearanceSettingsPatch, CommandStyle, ConfigScope, ConfigService, DEFAULT_SCHEMA_URL,
    EditorSettingsPatch, KeybindPreset, KeybindingSettingsPatch, LgtmConfig, LgtmConfigPatch,
    LineWrap, ThemeMode,
};
use serde_json::json;
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

const CARGO_TOML_TEMPLATE: &str = "\
[package]
name = \"sample\"
version = \"0.1.0\"
";

const GLOBAL_DARK_OFF_JSON: &str = r#"{
  "appearance": {
    "theme": "dark"
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#;

const GLOBAL_PARTIAL_INVALID_JSON: &str = r#"{
  "appearance": {
    "theme": "invalid-theme",
    "unknownGlobalProp": true
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#;

const GLOBAL_SYNTAX_ERROR_JSON: &str = "{ \"appearance\": broken }";

const GLOBAL_FULL_OVERRIDE_JSON: &str = r#"{
  "$schema": "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json",
  "appearance": {
    "theme": "dark"
  },
  "workspace": {
    "activeRoot": "/tmp/ws-global",
    "recentRoots": ["/tmp/ws-global"]
  },
  "editor": {
    "lineWrapping": 100
  },
  "lsp": {
    "autoInstall": false,
    "requestTimeoutMs": 7000
  }
}
"#;

const PROJECT_LIGHT_TIMEOUT_JSON: &str = r#"{
  "$schema": "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json",
  "appearance": {
    "theme": "light"
  },
  "lsp": {
    "requestTimeoutMs": 9000
  }
}
"#;

const GLOBAL_DARK_AUTO_JSON: &str = r#"{
  "appearance": {
    "theme": "dark"
  },
  "editor": {
    "lineWrapping": "auto"
  }
}
"#;

const PROJECT_UNKNOWN_FIELD_OFF_JSON: &str = r#"{
  "appearance": {
    "unknownLocalField": true
  },
  "editor": {
    "lineWrapping": "off"
  }
}
"#;

const GLOBAL_INVALID_THEME_TIMEOUT_JSON: &str = r#"{
  "appearance": {
    "theme": "invalid-theme"
  },
  "lsp": {
    "requestTimeoutMs": 8000
  }
}
"#;

const PROJECT_INVALID_WRAP_JSON: &str = r#"{
  "appearance": {
    "unknownLocalField": true
  },
  "editor": {
    "lineWrapping": "invalid-wrap"
  }
}
"#;

const GLOBAL_KEYBINDINGS_JSON: &str = r#"{
  "keybindings": {
    "commandStyle": "palette",
    "preset": "vim",
    "leaderOnly": true
  }
}
"#;

const PROJECT_KEYBINDINGS_JSON: &str = r#"{
  "keybindings": {
    "preset": "kakoune",
    "clickToInsert": false
  }
}
"#;

const GLOBAL_DARK_COL100_JSON: &str = r#"{
  "appearance": {
    "theme": "dark"
  },
  "editor": {
    "lineWrapping": 100
  }
}
"#;

const GLOBAL_DARK_MENU_JSON: &str = r#"{
  "appearance": {
    "theme": "dark"
  },
  "keybindings": {
    "commandStyle": "menu"
  }
}
"#;

const PROJECT_PALETTE_JSON: &str = r#"{
  "keybindings": {
    "commandStyle": "palette"
  }
}
"#;

fn render_config_summary(config: &LgtmConfig) -> String {
    serde_json::to_string_pretty(&json!({
        "appearance": &config.appearance,
        "workspace": &config.workspace,
        "editor": &config.editor,
        "keybindings": &config.keybindings,
        "lsp": &config.lsp,
    }))
    .unwrap()
}

struct ExpectedConfig<'a> {
    theme: &'a str,
    active_root: Option<&'a str>,
    recent_roots: &'a [&'a str],
    line_wrapping: serde_json::Value,
    command_style: &'a str,
    preset: &'a str,
    leader_only: bool,
    pane_outline: bool,
    click_to_insert: bool,
    auto_install: bool,
    request_timeout_ms: u64,
}

impl Default for ExpectedConfig<'_> {
    fn default() -> Self {
        Self {
            theme: "system",
            active_root: None,
            recent_roots: &[],
            line_wrapping: json!("auto"),
            command_style: "menu",
            preset: "helix",
            leader_only: false,
            pane_outline: true,
            click_to_insert: true,
            auto_install: true,
            request_timeout_ms: 15000,
        }
    }
}

impl ExpectedConfig<'_> {
    fn render(&self) -> String {
        let mut workspace = serde_json::Map::new();
        if let Some(root) = self.active_root {
            workspace.insert("activeRoot".to_string(), json!(root));
        }
        if !self.recent_roots.is_empty() {
            workspace.insert("recentRoots".to_string(), json!(self.recent_roots));
        }
        serde_json::to_string_pretty(&json!({
            "appearance": {
                "theme": self.theme,
            },
            "workspace": workspace,
            "editor": {
                "lineWrapping": self.line_wrapping,
            },
            "keybindings": {
                "commandStyle": self.command_style,
                "preset": self.preset,
                "leaderOnly": self.leader_only,
                "paneOutline": self.pane_outline,
                "clickToInsert": self.click_to_insert,
            },
            "lsp": {
                "enabled": true,
                "autoInstall": self.auto_install,
                "requestTimeoutMs": self.request_timeout_ms,
            },
        }))
        .unwrap()
    }
}

struct ConfigFixture {
    _temp: TempDir,
    home: PathBuf,
    workspace_root: PathBuf,
    service: ConfigService,
}

impl ConfigFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let service = ConfigService::from_home(home.clone());
        Self {
            _temp: temp,
            home,
            workspace_root,
            service,
        }
    }

    fn with_global(self, raw_json: &str) -> Self {
        self.write_global(raw_json);
        self
    }

    fn with_project(self, raw_json: &str) -> Self {
        self.write_project(raw_json);
        self
    }

    fn write_global(&self, raw_json: &str) {
        let dir = self.home.join(".lgtm");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), raw_json).unwrap();
    }

    fn write_project(&self, raw_json: &str) {
        let dir = self.workspace_root.join(".lgtm");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), raw_json).unwrap();
    }

    fn project_config_path(&self) -> PathBuf {
        self.workspace_root.join(".lgtm/config.json")
    }
}

// tests -----------------------------------------------------------------------

mod given_no_global_config {
    use super::*;

    mod when_resolving_config_snapshot {
        use super::*;

        #[test]
        fn should_return_default_config_and_presets() {
            // Arrange
            let fixture = ConfigFixture::new();

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();
            let config = fixture.service.get_config(&fixture.workspace_root).unwrap();

            // Assert
            assert_eq!(snapshot.effective, config);
            assert_eq!(snapshot.effective, LgtmConfig::default());
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig::default().render()
            );
            assert!(snapshot.project.is_none());
        }
    }

    mod when_ensuring_workspace_dir {
        use super::*;

        #[test]
        fn should_create_dot_lgtm_directory_and_register_git_exclude() {
            // Arrange
            let fixture = ConfigFixture::new();
            fs::create_dir_all(fixture.workspace_root.join(".git/info")).unwrap();
            fs::write(
                fixture.workspace_root.join("Cargo.toml"),
                CARGO_TOML_TEMPLATE,
            )
            .unwrap();

            // Act
            let lgtm_dir = fixture
                .service
                .ensure_workspace_dir(&fixture.workspace_root)
                .unwrap();
            let exclude_contents =
                fs::read_to_string(fixture.workspace_root.join(".git/info/exclude")).unwrap();

            // Assert
            assert_eq!(lgtm_dir, fixture.workspace_root.join(".lgtm"));
            assert_eq!(
                fixture.service.get_lgtm_home_dir(),
                fixture.home.join(".lgtm")
            );
            assert!(lgtm_dir.is_dir());
            assert!(exclude_contents.lines().any(|line| line.trim() == ".lgtm/"));
        }
    }

    mod when_generating_json_schema {
        use super::*;

        #[test]
        fn should_match_checked_in_schemas_config_schema_json() {
            // Arrange
            let repo_schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .expect("expected workspace root")
                .join("schemas/config.schema.json");

            // Act
            let generated = LgtmConfig::json_schema_pretty();
            if std::env::var_os("LGTM_WRITE_SCHEMA").is_some() {
                if let Some(parent) = repo_schema_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&repo_schema_path, &generated).unwrap();
            }
            let checked_in = fs::read_to_string(&repo_schema_path)
                .expect("expected schemas/config.schema.json at repo root");

            // Assert
            assert_eq!(
                DEFAULT_SCHEMA_URL,
                "https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json"
            );
            assert_eq!(checked_in, generated);
        }
    }
}

mod given_global_config_and_no_local_config {
    use super::*;

    mod when_global_config_is_valid {
        use super::*;

        #[test]
        fn should_override_defaults_with_global_settings() {
            // Arrange
            let fixture = ConfigFixture::new().with_global(GLOBAL_DARK_OFF_JSON);

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    theme: "dark",
                    line_wrapping: json!("off"),
                    ..ExpectedConfig::default()
                }
                .render()
            );
            assert!(snapshot.project.is_none());
        }
    }

    mod when_global_config_has_errors {
        use super::*;

        #[test]
        fn should_ignore_invalid_fields_and_fall_back_to_defaults() {
            // Arrange
            let fixture = ConfigFixture::new().with_global(GLOBAL_PARTIAL_INVALID_JSON);

            // Act: unknown field and invalid enum value alongside a valid editor setting
            let partial_snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Act: completely malformed JSON syntax
            fixture.write_global(GLOBAL_SYNTAX_ERROR_JSON);
            let syntax_fallback = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&partial_snapshot.effective),
                ExpectedConfig {
                    line_wrapping: json!("off"),
                    ..ExpectedConfig::default()
                }
                .render()
            );
            assert_eq!(syntax_fallback.effective, LgtmConfig::default());
        }
    }
}

mod given_global_and_local_configs {
    use super::*;

    mod when_both_configs_are_valid {
        use super::*;

        #[test]
        fn should_recursively_merge_local_overrides_over_global_and_defaults() {
            // Arrange
            let fixture = ConfigFixture::new()
                .with_global(GLOBAL_FULL_OVERRIDE_JSON)
                .with_project(PROJECT_LIGHT_TIMEOUT_JSON);

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();
            let config = fixture.service.get_config(&fixture.workspace_root).unwrap();

            // Assert
            assert_eq!(snapshot.effective, config);
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    theme: "light",
                    active_root: Some("/tmp/ws-global"),
                    recent_roots: &["/tmp/ws-global"],
                    line_wrapping: json!(100),
                    auto_install: false,
                    request_timeout_ms: 9000,
                    ..ExpectedConfig::default()
                }
                .render()
            );
        }
    }

    mod when_local_config_has_errors {
        use super::*;

        #[test]
        fn should_ignore_invalid_local_fields_and_preserve_valid_local_and_global_settings() {
            // Arrange
            let fixture = ConfigFixture::new()
                .with_global(GLOBAL_DARK_AUTO_JSON)
                .with_project(PROJECT_UNKNOWN_FIELD_OFF_JSON);

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    theme: "dark",
                    line_wrapping: json!("off"),
                    ..ExpectedConfig::default()
                }
                .render()
            );
        }
    }

    mod when_both_global_and_local_configs_have_errors {
        use super::*;

        #[test]
        fn should_preserve_any_valid_fields_and_fall_back_to_default_config() {
            // Arrange
            let fixture = ConfigFixture::new()
                .with_global(GLOBAL_INVALID_THEME_TIMEOUT_JSON)
                .with_project(PROJECT_INVALID_WRAP_JSON);

            // Act: both files have invalid properties alongside valid ones
            let partial = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Act: both files are completely corrupted JSON
            fixture.write_global("{ corrupted global");
            fixture.write_project("{ corrupted local");
            let total_fallback = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&partial.effective),
                ExpectedConfig {
                    request_timeout_ms: 8000,
                    ..ExpectedConfig::default()
                }
                .render()
            );
            assert_eq!(total_fallback.effective, LgtmConfig::default());
        }
    }
}

mod given_a_config_service {
    use super::*;

    mod when_resolving_keybinding_settings {
        use super::*;

        #[test]
        fn should_default_command_style_to_menu_and_preset_to_helix() {
            // Arrange
            let fixture = ConfigFixture::new();

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig::default().render()
            );
        }

        #[test]
        fn should_merge_project_keybinding_overrides_over_global_settings() {
            // Arrange
            let fixture = ConfigFixture::new()
                .with_global(GLOBAL_KEYBINDINGS_JSON)
                .with_project(PROJECT_KEYBINDINGS_JSON);

            // Act
            let snapshot = fixture
                .service
                .get_config_snapshot(Some(&fixture.workspace_root))
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    command_style: "palette",
                    preset: "kakoune",
                    leader_only: true,
                    click_to_insert: false,
                    ..ExpectedConfig::default()
                }
                .render()
            );
        }
    }

    mod when_updating_config_with_a_patch {
        use super::*;

        #[test]
        fn should_write_patch_to_global_config_by_default_and_preserve_sibling_fields() {
            // Arrange
            let fixture = ConfigFixture::new().with_global(GLOBAL_DARK_COL100_JSON);

            // Act
            let snapshot = fixture
                .service
                .update_config(
                    Some(&fixture.workspace_root),
                    LgtmConfigPatch {
                        keybindings: Some(KeybindingSettingsPatch {
                            command_style: Some(CommandStyle::Palette),
                            preset: Some(KeybindPreset::Vim),
                            ..KeybindingSettingsPatch::default()
                        }),
                        ..LgtmConfigPatch::default()
                    },
                    None,
                )
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    theme: "dark",
                    line_wrapping: json!(100),
                    command_style: "palette",
                    preset: "vim",
                    ..ExpectedConfig::default()
                }
                .render()
            );
            assert!(!fixture.project_config_path().exists());
        }

        #[test]
        fn should_write_patch_to_project_config_when_leaf_is_already_overridden_in_project() {
            // Arrange
            let fixture = ConfigFixture::new()
                .with_global(GLOBAL_DARK_MENU_JSON)
                .with_project(PROJECT_PALETTE_JSON);

            // Act
            let snapshot = fixture
                .service
                .update_config(
                    Some(&fixture.workspace_root),
                    LgtmConfigPatch {
                        appearance: Some(AppearanceSettingsPatch {
                            theme: Some(ThemeMode::Light),
                        }),
                        editor: Some(EditorSettingsPatch {
                            line_wrapping: Some(LineWrap::Off),
                        }),
                        keybindings: Some(KeybindingSettingsPatch {
                            command_style: Some(CommandStyle::Menu),
                            preset: Some(KeybindPreset::Kakoune),
                            ..KeybindingSettingsPatch::default()
                        }),
                        ..LgtmConfigPatch::default()
                    },
                    None,
                )
                .unwrap();
            let explicit_project = fixture
                .service
                .update_config(
                    Some(&fixture.workspace_root),
                    LgtmConfigPatch {
                        keybindings: Some(KeybindingSettingsPatch {
                            leader_only: Some(true),
                            ..KeybindingSettingsPatch::default()
                        }),
                        ..LgtmConfigPatch::default()
                    },
                    Some(ConfigScope::Project),
                )
                .unwrap();

            // Assert
            assert_eq!(
                render_config_summary(&snapshot.effective),
                ExpectedConfig {
                    theme: "light",
                    line_wrapping: json!("off"),
                    preset: "kakoune",
                    ..ExpectedConfig::default()
                }
                .render()
            );
            assert_eq!(
                serde_json::to_string_pretty(&snapshot.global).unwrap(),
                serde_json::to_string_pretty(&json!({
                    "$schema": DEFAULT_SCHEMA_URL,
                    "appearance": { "theme": "light" },
                    "editor": { "lineWrapping": "off" },
                    "keybindings": {
                        "commandStyle": "menu",
                        "preset": "kakoune"
                    }
                }))
                .unwrap()
            );
            assert_eq!(
                serde_json::to_string_pretty(&snapshot.project).unwrap(),
                serde_json::to_string_pretty(&json!({
                    "$schema": DEFAULT_SCHEMA_URL,
                    "keybindings": {
                        "commandStyle": "menu"
                    }
                }))
                .unwrap()
            );
            assert!(explicit_project.effective.keybindings.leader_only);
        }
    }
}
