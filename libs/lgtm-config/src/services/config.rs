use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::error::{LgtmConfigError, Result};
use crate::models::config::{LanguageConfig, LgtmConfig, LgtmConfigPatch, ResolvedConfigSnapshot};

/// Environment variable passing the startup workspace directory from `lgtm-cli` to `lgtm-desktop`.
pub const LGTM_WORKSPACE_ENV: &str = "LGTM_WORKSPACE";

/// Environment variable overriding the `lgtm-desktop` binary path or executable name.
pub const LGTM_DESKTOP_BIN_ENV: &str = "LGTM_DESKTOP_BIN";

#[derive(Debug, Clone, Copy)]
enum ConfigScope {
    Global,
    Project,
}

/// Resolves global (`~/.lgtm/config.json`) and workspace (`<workspace>/.lgtm/config.json`) configuration, JSON Schema files, and `.lgtm/` directory layout.
#[derive(Debug, Clone, Default)]
pub struct ConfigService {
    global_config_override: Option<PathBuf>,
}

impl ConfigService {
    /// Creates a `ConfigService` that resolves the global config from `LGTM_HOME` or `~/.lgtm/config.json`.
    pub fn new() -> Self {
        Self {
            global_config_override: None,
        }
    }

    /// Creates a `ConfigService` rooted at `<home>/.lgtm/config.json`.
    pub fn from_home(home: PathBuf) -> Self {
        Self::with_global_config_path(home.join(".lgtm").join("config.json"))
    }

    /// Creates a `ConfigService` with an explicit global configuration file path.
    pub fn with_global_config_path(path: PathBuf) -> Self {
        Self {
            global_config_override: Some(path),
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns the resolved LGTM home directory (`LGTM_HOME` or `~/.lgtm`).
    pub fn get_lgtm_home_dir(&self) -> PathBuf {
        if let Some(override_path) = &self.global_config_override {
            return override_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".lgtm"));
        }
        if let Ok(lgtm_home) = std::env::var("LGTM_HOME") {
            let trimmed = lgtm_home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lgtm")
    }

    /// Returns the resolved path to the global `.lgtm/config.json` file.
    pub fn get_global_config_path(&self) -> PathBuf {
        self.global_config_override
            .clone()
            .unwrap_or_else(|| self.get_lgtm_home_dir().join("config.json"))
    }

    /// Returns the project configuration path (`<workspace>/.lgtm/config.json`).
    pub fn get_project_config_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".lgtm").join("config.json")
    }

    /// Returns a designated cache file path inside `<workspace>/.lgtm/cache/<file_name>`.
    pub fn get_workspace_cache_path(workspace_root: &Path, file_name: &str) -> PathBuf {
        workspace_root.join(".lgtm").join("cache").join(file_name)
    }

    /// Loads and merges built-in runtime presets, the global config, and the workspace project config.
    pub fn get_config(&self, workspace_root: &Path) -> Result<LgtmConfig> {
        Ok(self.get_config_snapshot(Some(workspace_root))?.effective)
    }

    /// Loads and deep-merges nested `LgtmConfig` across built-in defaults, global `~/.lgtm/config.json`, and optional workspace `<workspace>/.lgtm/config.json`.
    pub fn get_config_snapshot(
        &self,
        workspace_root: Option<&Path>,
    ) -> Result<ResolvedConfigSnapshot> {
        let global_path = self.get_global_config_path();
        let global_patch: LgtmConfigPatch = if global_path.is_file() {
            Self::read_scoped_file(ConfigScope::Global, &global_path)?
        } else {
            LgtmConfigPatch::default()
        };

        let (project_path, project_patch) = match workspace_root {
            Some(ws) => {
                let p_path = Self::get_project_config_path(ws);
                let patch = if p_path.is_file() {
                    Some(Self::read_scoped_file(ConfigScope::Project, &p_path)?)
                } else {
                    None
                };
                (p_path, patch)
            }
            None => (PathBuf::new(), None),
        };

        let effective = Self::resolve_effective_config(&global_patch, project_patch.as_ref());
        let resolved_global = std::fs::canonicalize(&global_path).unwrap_or(global_path);
        let resolved_project = std::fs::canonicalize(&project_path).unwrap_or(project_path);

        Ok(ResolvedConfigSnapshot {
            effective,
            global: global_patch,
            project: project_patch,
            global_path: resolved_global.to_string_lossy().to_string(),
            project_path: resolved_project.to_string_lossy().to_string(),
        })
    }

    /// Ensures `<workspace>/.lgtm/` exists and registers `.lgtm/` in `<git-dir>/info/exclude`.
    pub fn ensure_workspace_dir(&self, workspace_root: &Path) -> Result<PathBuf> {
        let lgtm_dir = workspace_root.join(".lgtm");
        std::fs::create_dir_all(&lgtm_dir).map_err(|source| LgtmConfigError::ProjectConfigIo {
            path: lgtm_dir.clone(),
            source,
        })?;

        Self::ensure_git_exclude(workspace_root);

        Ok(lgtm_dir)
    }

    /// Walks up from `file_path` toward `workspace_root` to find the nearest marker for `language`.
    pub fn detect_project_root(
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

    /// Scans up to two directory levels under `workspace_root` to identify enabled workspace languages.
    pub fn detect_workspace_languages(config: &LgtmConfig, workspace_root: &Path) -> Vec<String> {
        let mut detected = BTreeSet::new();
        Self::scan_directory_languages(config, workspace_root, &mut detected, 2);
        detected.into_iter().collect()
    }

    /// Validates that `path` (or the current directory when `None`) is an existing directory and returns its canonical path.
    pub fn resolve_workspace_dir(path: Option<&Path>) -> Result<PathBuf> {
        let target = path.unwrap_or_else(|| Path::new("."));
        if !target.exists() {
            return Err(LgtmConfigError::WorkspaceDirectoryNotFound {
                path: target.to_path_buf(),
            });
        }
        if !target.is_dir() {
            return Err(LgtmConfigError::WorkspacePathNotDirectory {
                path: target.to_path_buf(),
            });
        }
        std::fs::canonicalize(target).map_err(|source| LgtmConfigError::WorkspaceCanonicalize {
            path: target.to_path_buf(),
            source,
        })
    }

    /// Resolves the workspace directory passed to `lgtm-desktop` via `LGTM_WORKSPACE` or the first non-flag CLI argument.
    pub fn resolve_desktop_workspace() -> Option<PathBuf> {
        if let Some(env_path) = std::env::var_os(LGTM_WORKSPACE_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
        {
            return Some(env_path);
        }
        let first = std::env::args_os().nth(1)?;
        if first.to_string_lossy().starts_with('-') {
            return None;
        }
        Some(PathBuf::from(first))
    }

    /// Resolves the `lgtm-desktop` executable from `LGTM_DESKTOP_BIN`, a sibling binary, `PATH`, or macOS `.app` bundles.
    pub fn resolve_desktop_executable() -> Result<PathBuf> {
        if let Some(configured) = std::env::var_os(LGTM_DESKTOP_BIN_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if configured.is_file() {
                return Ok(std::fs::canonicalize(&configured).unwrap_or(configured));
            }
            if let Some(found) = Self::find_in_path(&configured) {
                return Ok(found);
            }
            return Err(LgtmConfigError::DesktopExecutableNotFound {
                detail: format!(
                    "Configured {LGTM_DESKTOP_BIN_ENV} '{}' does not exist",
                    configured.display()
                ),
            });
        }

        let bin_name = Self::desktop_binary_name();
        if let Ok(current_exe) = std::env::current_exe()
            && let Some(exe_dir) = current_exe.parent()
        {
            let sibling = exe_dir.join(bin_name);
            if sibling.is_file() {
                return Ok(std::fs::canonicalize(&sibling).unwrap_or(sibling));
            }
        }

        if let Some(found) = Self::find_in_path(Path::new(bin_name)) {
            return Ok(found);
        }

        for candidate in Self::macos_app_candidates() {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        Err(LgtmConfigError::DesktopExecutableNotFound {
            detail: format!(
                "Could not find lgtm-desktop executable; set {LGTM_DESKTOP_BIN_ENV} or install LGTM Desktop"
            ),
        })
    }

    // Private -----------------------------------------------------------------

    fn resolve_effective_config(
        global_patch: &LgtmConfigPatch,
        project_patch: Option<&LgtmConfigPatch>,
    ) -> LgtmConfig {
        let mut config = LgtmConfig::default();
        config.apply_patch(global_patch);
        if let Some(proj) = project_patch {
            config.apply_patch(proj);
        }
        config
    }

    fn read_scoped_file<T: DeserializeOwned + Default>(
        scope: ConfigScope,
        path: &Path,
    ) -> Result<T> {
        let raw = std::fs::read_to_string(path)
            .map_err(|source| Self::scoped_io_error(scope, path, source))?;
        if raw.trim().is_empty() {
            return Ok(T::default());
        }
        serde_json::from_str::<T>(&raw)
            .map_err(|err| Self::scoped_parse_error(scope, path, err.to_string()))
    }

    fn scoped_io_error(scope: ConfigScope, path: &Path, source: std::io::Error) -> LgtmConfigError {
        match scope {
            ConfigScope::Global => LgtmConfigError::GlobalConfigIo {
                path: path.to_path_buf(),
                source,
            },
            ConfigScope::Project => LgtmConfigError::ProjectConfigIo {
                path: path.to_path_buf(),
                source,
            },
        }
    }

    fn scoped_parse_error(scope: ConfigScope, path: &Path, message: String) -> LgtmConfigError {
        match scope {
            ConfigScope::Global => LgtmConfigError::GlobalConfigParse {
                path: path.to_path_buf(),
                message,
            },
            ConfigScope::Project => LgtmConfigError::ProjectConfigParse {
                path: path.to_path_buf(),
                message,
            },
        }
    }

    fn scan_directory_languages(
        config: &LgtmConfig,
        dir: &Path,
        detected: &mut BTreeSet<String>,
        remaining_depth: usize,
    ) {
        for (id, cfg) in &config.languages {
            if cfg.enabled
                && cfg
                    .root_markers
                    .iter()
                    .any(|marker| dir.join(marker).exists())
            {
                detected.insert(id.clone());
            }
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some((lang_id, _)) = config.detect_file_language(&path) {
                    if lang_id == "json" {
                        continue;
                    }
                    detected.insert(lang_id);
                }
            } else if remaining_depth > 0 && path.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                    continue;
                }
                Self::scan_directory_languages(config, &path, detected, remaining_depth - 1);
            }
        }
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

    fn desktop_binary_name() -> &'static str {
        if cfg!(windows) {
            "lgtm-desktop.exe"
        } else {
            "lgtm-desktop"
        }
    }

    fn find_in_path(binary: &Path) -> Option<PathBuf> {
        if binary.components().count() > 1 {
            return None;
        }
        let path_var = std::env::var_os("PATH")?;
        std::env::split_paths(&path_var)
            .map(|dir| dir.join(binary))
            .find(|candidate| candidate.is_file())
    }

    fn macos_app_candidates() -> Vec<PathBuf> {
        let mut candidates = vec![
            PathBuf::from("/Applications/lgtm-desktop.app/Contents/MacOS/lgtm-desktop"),
            PathBuf::from("/Applications/LGTM.app/Contents/MacOS/lgtm-desktop"),
        ];
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("Applications/lgtm-desktop.app/Contents/MacOS/lgtm-desktop"));
            candidates.push(home.join("Applications/LGTM.app/Contents/MacOS/lgtm-desktop"));
        }
        candidates
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_git_backed_rust_workspace {
        use super::*;

        mod when_ensuring_workspace_dir {
            use super::*;

            #[test]
            fn should_create_dot_lgtm_dir_and_register_git_exclude_idempotently() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                std::fs::create_dir_all(workspace_root.join(".git/info")).unwrap();
                let service = ConfigService::from_home(workspace_root.to_path_buf());

                // Act
                let lgtm_dir = service.ensure_workspace_dir(workspace_root).unwrap();
                let _ = service.ensure_workspace_dir(workspace_root).unwrap();
                let exclude_contents =
                    std::fs::read_to_string(workspace_root.join(".git/info/exclude")).unwrap();

                // Assert
                assert_eq!(lgtm_dir, workspace_root.join(".lgtm"));
                assert!(lgtm_dir.is_dir());
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
                let global_cfg = workspace_root.join("global.json");
                std::fs::write(
                    &global_cfg,
                    r#"{
  "lsp": {
    "enabled": true,
    "auto_install": false,
    "request_timeout_ms": 7000
  }
}
"#,
                )
                .unwrap();
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.json"),
                    "{\n  \"lsp\": {\n    \"enabled\": true,\n    \"auto_install\": true,\n    \"request_timeout_ms\": 9000\n  }\n}\n",
                )
                .unwrap();
                let service = ConfigService::with_global_config_path(global_cfg);

                // Act
                let config = service.get_config(workspace_root).unwrap();

                // Assert
                assert!(config.lsp.auto_install);
                assert_eq!(config.lsp.request_timeout_ms, 9000);
            }
        }

        mod when_detecting_project_root_and_workspace_languages {
            use super::*;

            #[test]
            fn should_walk_up_to_nearest_root_marker_and_list_sorted_languages() {
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
                std::fs::write(workspace_root.join("package.json"), "{}\n").unwrap();
                let config = LgtmConfig::default();
                let rust_cfg = config.languages.get("rust").unwrap();

                // Act
                let (project_root, marker) =
                    ConfigService::detect_project_root(workspace_root, &file_path, rust_cfg);
                let languages = ConfigService::detect_workspace_languages(&config, workspace_root);

                // Assert
                assert_eq!(project_root, crate_root);
                assert_eq!(marker, Some("Cargo.toml".to_string()));
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

        mod when_resolving_workspace_dir {
            use super::*;

            #[test]
            fn should_canonicalize_existing_directory() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let nested = temp.path().join("project");
                std::fs::create_dir_all(&nested).unwrap();

                // Act
                let resolved = ConfigService::resolve_workspace_dir(Some(&nested)).unwrap();

                // Assert
                assert_eq!(resolved, std::fs::canonicalize(&nested).unwrap());
            }

            #[test]
            fn should_reject_nonexistent_directory() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let missing = temp.path().join("missing");

                // Act
                let err = ConfigService::resolve_workspace_dir(Some(&missing)).unwrap_err();

                // Assert
                assert!(matches!(
                    err,
                    LgtmConfigError::WorkspaceDirectoryNotFound { .. }
                ));
            }

            #[test]
            fn should_reject_file_path() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let file_path = temp.path().join("README.md");
                std::fs::write(&file_path, "# demo\n").unwrap();

                // Act
                let err = ConfigService::resolve_workspace_dir(Some(&file_path)).unwrap_err();

                // Assert
                assert!(matches!(
                    err,
                    LgtmConfigError::WorkspacePathNotDirectory { .. }
                ));
            }
        }
    }
}
