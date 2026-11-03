use std::path::{Path, PathBuf};

use crate::error::{ConfigError, Result};
use crate::models::config::LgtmConfig;
use crate::models::mise::GeneratedMiseFile;

#[derive(Debug, Clone, Default)]
pub struct ConfigService {
    global_config_override: Option<PathBuf>,
}

impl ConfigService {
    pub fn new() -> Self {
        Self {
            global_config_override: None,
        }
    }

    pub fn with_global_config_path(path: PathBuf) -> Self {
        Self {
            global_config_override: Some(path),
        }
    }

    // Public ------------------------------------------------------------------

    pub fn get_global_config_path(&self) -> PathBuf {
        if let Some(override_path) = &self.global_config_override {
            return override_path.clone();
        }
        if let Ok(lgtm_home) = std::env::var("LGTM_HOME") {
            let trimmed = lgtm_home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("config.toml");
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lgtm")
            .join("config.toml")
    }

    pub fn get_project_config_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".lgtm").join("config.toml")
    }

    pub fn get_managed_mise_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".lgtm").join("mise.toml")
    }

    pub fn get_config(&self, workspace_root: &Path) -> Result<LgtmConfig> {
        let mut config = LgtmConfig::default();

        let global_path = self.get_global_config_path();
        if global_path.is_file() {
            let overlay = Self::read_config_file(&global_path)?;
            config.merge(overlay);
        }

        let project_path = Self::get_project_config_path(workspace_root);
        if project_path.is_file() {
            let overlay = Self::read_config_file(&project_path)?;
            config.merge(overlay);
        }

        Ok(config)
    }

    pub fn ensure_workspace_layout(&self, workspace_root: &Path) -> Result<PathBuf> {
        let lgtm_dir = workspace_root.join(".lgtm");
        std::fs::create_dir_all(&lgtm_dir).map_err(|source| ConfigError::Io {
            path: lgtm_dir.clone(),
            source,
        })?;

        Self::ensure_git_exclude(workspace_root);

        let config = self.get_config(workspace_root)?;
        let detected_languages = config.detect_workspace_languages(workspace_root);
        let generated = GeneratedMiseFile::from_config(&config, &detected_languages);
        let rendered = generated.render_toml()?;
        let mise_path = Self::get_managed_mise_path(workspace_root);

        let needs_write = match std::fs::read_to_string(&mise_path) {
            Ok(existing) => existing != rendered,
            Err(_) => true,
        };

        if needs_write {
            std::fs::write(&mise_path, rendered).map_err(|source| ConfigError::Io {
                path: mise_path.clone(),
                source,
            })?;
        }

        Ok(mise_path)
    }

    // Private -----------------------------------------------------------------

    fn read_config_file(path: &Path) -> Result<LgtmConfig> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ConfigError::ParseToml {
            path: path.to_path_buf(),
            source,
        })
    }

    fn ensure_git_exclude(workspace_root: &Path) {
        let Some(git_dir) = Self::resolve_git_dir(workspace_root) else {
            return;
        };
        let info_dir = git_dir.join("info");
        if std::fs::create_dir_all(&info_dir).is_err() {
            return;
        }
        let exclude_path = info_dir.join("exclude");
        let existing = std::fs::read_to_string(&exclude_path).unwrap_or_default();
        if existing
            .lines()
            .any(|line| line.trim() == ".lgtm/" || line.trim() == ".lgtm")
        {
            return;
        }
        let mut updated = existing;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(".lgtm/\n");
        let _ = std::fs::write(exclude_path, updated);
    }

    fn resolve_git_dir(workspace_root: &Path) -> Option<PathBuf> {
        let dot_git = workspace_root.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            let content = std::fs::read_to_string(&dot_git).ok()?;
            let gitdir = content.strip_prefix("gitdir:")?.trim();
            let resolved = Path::new(gitdir);
            if resolved.is_absolute() {
                return Some(resolved.to_path_buf());
            }
            return Some(workspace_root.join(resolved));
        }
        None
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_git_backed_rust_workspace {
        use super::*;

        mod when_ensuring_workspace_layout {
            use super::*;

            #[test]
            fn should_generate_managed_mise_file_and_register_git_exclude_idempotently() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                let global_cfg = workspace_root.join("global-config.toml");
                std::fs::create_dir_all(workspace_root.join(".git/info")).unwrap();
                std::fs::write(
                    workspace_root.join("Cargo.toml"),
                    "[package]\nname = \"demo\"\n",
                )
                .unwrap();
                let service = ConfigService::with_global_config_path(global_cfg);

                // Act
                let mise_path = service.ensure_workspace_layout(workspace_root).unwrap();
                let _ = service.ensure_workspace_layout(workspace_root).unwrap();
                let mise_contents = std::fs::read_to_string(&mise_path).unwrap();
                let exclude_contents =
                    std::fs::read_to_string(workspace_root.join(".git/info/exclude")).unwrap();

                // Assert
                assert!(mise_contents.contains("rust-analyzer = \"latest\""));
                assert!(mise_contents.contains("cargo check"));
                let lgtm_lines: Vec<&str> = exclude_contents
                    .lines()
                    .filter(|line| line.trim() == ".lgtm/")
                    .collect();
                assert_eq!(lgtm_lines.len(), 1);
            }
        }

        mod when_project_config_overrides_global_config {
            use super::*;

            #[test]
            fn should_merge_global_and_project_settings() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                let global_cfg = workspace_root.join("global.toml");
                std::fs::write(
                    &global_cfg,
                    "[lsp]\nenabled = true\nauto_install = false\nrequest_timeout_ms = 7000\n\n[mise.tools]\nfd = \"latest\"\n",
                )
                .unwrap();
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.toml"),
                    "[lsp]\nenabled = true\nauto_install = true\nrequest_timeout_ms = 9000\n\n[mise.tasks.custom]\ndescription = \"Custom\"\nrun = \"echo custom\"\n",
                )
                .unwrap();
                let service = ConfigService::with_global_config_path(global_cfg);

                // Act
                let config = service.get_config(workspace_root).unwrap();

                // Assert
                assert!(config.lsp.auto_install);
                assert_eq!(config.lsp.request_timeout_ms, 9000);
                assert_eq!(config.mise.tools.get("fd"), Some(&"latest".to_string()));
                assert!(config.mise.tasks.contains_key("custom"));
            }
        }
    }
}
