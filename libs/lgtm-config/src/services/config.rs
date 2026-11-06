use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::error::{LgtmConfigError, Result};
use crate::models::config::{LanguageConfig, LgtmConfig, LgtmConfigFileOverlay};
use crate::models::settings::{
    ConfigDocument, ConfigFormat, ConfigScope, DEFAULT_SCHEMA_RELATIVE_PATH,
    ResolvedConfigSnapshot, UserSettings, UserSettingsPatch,
};

/// Environment variable passing the startup workspace directory from `lgtm-cli` to `lgtm-desktop`.
pub const LGTM_WORKSPACE_ENV: &str = "LGTM_WORKSPACE";

/// Environment variable overriding the `lgtm-desktop` binary path or executable name.
pub const LGTM_DESKTOP_BIN_ENV: &str = "LGTM_DESKTOP_BIN";

const CONFIG_CANDIDATES: [&str; 3] = ["config.json", "config.yaml", "config.yml"];

/// Resolves global and workspace-level LGTM configuration, JSON Schema files, and `.lgtm/` directory layout.
#[derive(Debug, Clone, Default)]
pub struct ConfigService {
    global_config_override: Option<PathBuf>,
}

impl ConfigService {
    /// Creates a `ConfigService` that resolves the global config from `LGTM_HOME` or `~/.lgtm/config.{json,yaml,yml}`.
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

    /// Returns the resolved path to the global `.lgtm/config.{json,yaml,yml}` file.
    pub fn get_global_config_path(&self) -> PathBuf {
        if let Some(override_path) = &self.global_config_override {
            if override_path.is_file() {
                return override_path.clone();
            }
            if let Some(parent) = override_path.parent()
                && let Some(found) = Self::find_existing_config_in_dir(parent)
            {
                return found;
            }
            return override_path.clone();
        }
        let home_dir = self.get_lgtm_home_dir();
        Self::find_existing_config_in_dir(&home_dir).unwrap_or_else(|| home_dir.join("config.json"))
    }

    /// Returns the project configuration path (`<workspace>/.lgtm/config.{json,yaml,yml}`).
    pub fn get_project_config_path(workspace_root: &Path) -> PathBuf {
        let dot_lgtm = workspace_root.join(".lgtm");
        Self::find_existing_config_in_dir(&dot_lgtm).unwrap_or_else(|| dot_lgtm.join("config.json"))
    }

    /// Returns the global JSON Schema path (`~/.lgtm/schemas/config.schema.json`).
    pub fn get_global_schema_path(&self) -> PathBuf {
        self.get_lgtm_home_dir()
            .join("schemas")
            .join("config.schema.json")
    }

    /// Returns the workspace JSON Schema path (`<workspace>/.lgtm/schemas/config.schema.json`).
    pub fn get_project_schema_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".lgtm")
            .join("schemas")
            .join("config.schema.json")
    }

    /// Returns the LGTM-managed Mise file path (`<workspace>/.lgtm/mise.toml`).
    pub fn get_managed_mise_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".lgtm").join("mise.toml")
    }

    /// Returns a designated cache file path inside `<workspace>/.lgtm/cache/<file_name>`.
    pub fn get_workspace_cache_path(workspace_root: &Path, file_name: &str) -> PathBuf {
        workspace_root.join(".lgtm").join("cache").join(file_name)
    }

    /// Writes `schemas/config.schema.json` (and `~/.lgtm/biome.json` if absent) for global and optional project `.lgtm` directories.
    pub fn ensure_schema_files(&self, workspace_root: Option<&Path>) -> Result<PathBuf> {
        let home_dir = self.get_lgtm_home_dir();
        let global_schema_path = self.get_global_schema_path();
        if let Some(parent) = global_schema_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| LgtmConfigError::GlobalConfigIo {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let schema_text = UserSettingsPatch::json_schema_pretty();
        std::fs::write(&global_schema_path, &schema_text).map_err(|source| {
            LgtmConfigError::GlobalConfigIo {
                path: global_schema_path.clone(),
                source,
            }
        })?;

        let biome_path = home_dir.join("biome.json");
        if !biome_path.exists() {
            let _ = std::fs::write(
                &biome_path,
                "{\n  \"$schema\": \"https://biomejs.dev/schemas/latest/schema.json\",\n  \"json\": {\n    \"parser\": {\n      \"allowComments\": false\n    }\n  }\n}\n",
            );
        }

        if let Some(ws) = workspace_root {
            let dot_lgtm = ws.join(".lgtm");
            if dot_lgtm.is_dir() {
                let project_schema_path = Self::get_project_schema_path(ws);
                if let Some(parent) = project_schema_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| {
                        LgtmConfigError::ProjectConfigIo {
                            path: parent.to_path_buf(),
                            source,
                        }
                    })?;
                }
                std::fs::write(&project_schema_path, &schema_text).map_err(|source| {
                    LgtmConfigError::ProjectConfigIo {
                        path: project_schema_path,
                        source,
                    }
                })?;
            }
        }

        Ok(global_schema_path)
    }

    /// Loads and merges built-in runtime presets, the global config, and the workspace project config.
    pub fn get_config(&self, workspace_root: &Path) -> Result<LgtmConfig> {
        let mut config = LgtmConfig::default();

        if let Some(script) = std::env::var_os("LGTM_E2E_LSP_SCRIPT")
            .map(|val| val.to_string_lossy().trim().to_string())
            .filter(|val| !val.is_empty())
        {
            for lang in config.languages.values_mut() {
                lang.command = "node".to_string();
                lang.args = vec![script.clone()];
                lang.mise_tools.clear();
            }
        }

        let global_path = self.get_global_config_path();
        if global_path.is_file() {
            let overlay: LgtmConfigFileOverlay =
                Self::read_scoped_file(ConfigScope::Global, &global_path)?;
            config.merge_file_overlay(overlay);
        }

        let project_path = Self::get_project_config_path(workspace_root);
        if project_path.is_file() {
            let overlay: LgtmConfigFileOverlay =
                Self::read_scoped_file(ConfigScope::Project, &project_path)?;
            config.merge_file_overlay(overlay);
        }

        Ok(config)
    }

    /// Loads and deep-merges nested `UserSettings` across defaults, global config, and optional workspace project config.
    pub fn get_settings_snapshot(
        &self,
        workspace_root: Option<&Path>,
    ) -> Result<ResolvedConfigSnapshot> {
        let global_path = self.get_global_config_path();
        let global_patch: UserSettingsPatch = if global_path.is_file() {
            Self::read_scoped_file(ConfigScope::Global, &global_path)?
        } else {
            UserSettingsPatch::default()
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

        let mut effective = UserSettings::default();
        effective.apply_patch(&global_patch);
        if let Some(proj) = &project_patch {
            effective.apply_patch(proj);
        }

        Ok(ResolvedConfigSnapshot {
            effective,
            global: global_patch,
            project: project_patch,
            global_path: global_path.to_string_lossy().to_string(),
            project_path: project_path.to_string_lossy().to_string(),
        })
    }

    /// Persists a nested `UserSettingsPatch` to `scope` (or routes each leaf by project override presence when `scope` is `None`).
    pub fn update_settings(
        &self,
        workspace_root: Option<&Path>,
        scope: Option<ConfigScope>,
        patch: UserSettingsPatch,
    ) -> Result<ResolvedConfigSnapshot> {
        let _ = self.ensure_schema_files(workspace_root)?;

        match scope {
            Some(ConfigScope::Global) => {
                let global_path = self.get_global_config_path();
                self.apply_and_write_scoped_patch(ConfigScope::Global, &global_path, patch)?;
            }
            Some(ConfigScope::Project) => {
                let Some(ws) = workspace_root else {
                    return self.get_settings_snapshot(None);
                };
                let _ = self.ensure_workspace_dir(ws)?;
                let _ = self.ensure_schema_files(Some(ws))?;
                let project_path = Self::get_project_config_path(ws);
                self.apply_and_write_scoped_patch(ConfigScope::Project, &project_path, patch)?;
            }
            None => {
                let existing_project = match workspace_root {
                    Some(ws) => {
                        let p_path = Self::get_project_config_path(ws);
                        if p_path.is_file() {
                            Some((
                                p_path.clone(),
                                Self::read_scoped_file::<UserSettingsPatch>(
                                    ConfigScope::Project,
                                    &p_path,
                                )?,
                            ))
                        } else {
                            None
                        }
                    }
                    None => None,
                };

                let (global_part, project_part) =
                    patch.split_by_project_overrides(existing_project.as_ref().map(|(_, p)| p));

                if let (Some(proj_patch), Some((project_path, _))) =
                    (project_part, existing_project)
                {
                    self.apply_and_write_scoped_patch(
                        ConfigScope::Project,
                        &project_path,
                        proj_patch,
                    )?;
                }
                if let Some(glob_patch) = global_part {
                    let global_path = self.get_global_config_path();
                    self.apply_and_write_scoped_patch(
                        ConfigScope::Global,
                        &global_path,
                        glob_patch,
                    )?;
                }
            }
        }

        self.get_settings_snapshot(workspace_root)
    }

    /// Ensures the target configuration document and its JSON Schema exist on disk and returns its raw text and LSP metadata.
    pub fn open_config_document(
        &self,
        workspace_root: Option<&Path>,
        scope: ConfigScope,
        preferred_format: Option<ConfigFormat>,
    ) -> Result<ConfigDocument> {
        let target_path = self.resolve_document_path(workspace_root, scope, preferred_format)?;
        let format = ConfigFormat::from_path(&target_path);
        self.prepare_scoped_storage(scope, workspace_root)?;
        if !target_path.is_file() {
            let default_body = UserSettingsPatch::default_document(format);
            Self::write_scoped_text(scope, &target_path, &default_body)?;
        }

        self.build_config_document(workspace_root, scope, &target_path)
    }

    /// Writes raw editor text to the `scope` configuration document and returns the updated document plus optional valid settings snapshot.
    pub fn write_config_document(
        &self,
        workspace_root: Option<&Path>,
        scope: ConfigScope,
        content: &str,
        preferred_format: Option<ConfigFormat>,
    ) -> Result<(ConfigDocument, Option<ResolvedConfigSnapshot>)> {
        let target_path = self.resolve_document_path(workspace_root, scope, preferred_format)?;
        self.prepare_scoped_storage(scope, workspace_root)?;
        Self::write_scoped_text(scope, &target_path, content)?;

        let doc = self.build_config_document(workspace_root, scope, &target_path)?;
        let snapshot = self.get_settings_snapshot(workspace_root).ok();
        Ok((doc, snapshot))
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

    fn find_existing_config_in_dir(dir: &Path) -> Option<PathBuf> {
        for candidate in CONFIG_CANDIDATES {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    fn resolve_document_path(
        &self,
        workspace_root: Option<&Path>,
        scope: ConfigScope,
        preferred_format: Option<ConfigFormat>,
    ) -> Result<PathBuf> {
        match scope {
            ConfigScope::Global => {
                let current = self.get_global_config_path();
                if current.is_file() && preferred_format.is_none() {
                    return Ok(current);
                }
                if let Some(format) = preferred_format {
                    let parent = current
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| self.get_lgtm_home_dir());
                    return Ok(parent.join(format.default_file_name()));
                }
                Ok(current)
            }
            ConfigScope::Project => {
                let Some(ws) = workspace_root else {
                    return Err(LgtmConfigError::ProjectConfigIo {
                        path: PathBuf::from(".lgtm/config.json"),
                        source: std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "no active workspace root",
                        ),
                    });
                };
                let current = Self::get_project_config_path(ws);
                if current.is_file() && preferred_format.is_none() {
                    return Ok(current);
                }
                if let Some(format) = preferred_format {
                    return Ok(ws.join(".lgtm").join(format.default_file_name()));
                }
                Ok(current)
            }
        }
    }

    fn build_config_document(
        &self,
        workspace_root: Option<&Path>,
        scope: ConfigScope,
        target_path: &Path,
    ) -> Result<ConfigDocument> {
        let canonical =
            std::fs::canonicalize(target_path).unwrap_or_else(|_| target_path.to_path_buf());
        let content = std::fs::read_to_string(&canonical).map_err(|source| match scope {
            ConfigScope::Global => LgtmConfigError::GlobalConfigIo {
                path: canonical.clone(),
                source,
            },
            ConfigScope::Project => LgtmConfigError::ProjectConfigIo {
                path: canonical.clone(),
                source,
            },
        })?;
        let format = ConfigFormat::from_path(&canonical);
        let file_name = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(format.default_file_name());
        let title = match scope {
            ConfigScope::Global => format!("Settings ({file_name})"),
            ConfigScope::Project => format!("Project Settings (.lgtm/{file_name})"),
        };
        let lsp_path = match workspace_root {
            Some(ws) => {
                let canonical_ws = std::fs::canonicalize(ws).unwrap_or_else(|_| ws.to_path_buf());
                canonical
                    .strip_prefix(&canonical_ws)
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| canonical.to_string_lossy().to_string())
            }
            None => canonical.to_string_lossy().to_string(),
        };
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let revision = format!("{:x}", hasher.finalize());

        Ok(ConfigDocument {
            scope,
            format,
            path: canonical.to_string_lossy().to_string(),
            lsp_path,
            title,
            content,
            revision,
            language_id: format.language_id().to_string(),
        })
    }

    fn prepare_scoped_storage(
        &self,
        scope: ConfigScope,
        workspace_root: Option<&Path>,
    ) -> Result<()> {
        match scope {
            ConfigScope::Global => {
                let _ = self.ensure_schema_files(workspace_root)?;
            }
            ConfigScope::Project => {
                if let Some(ws) = workspace_root {
                    let _ = self.ensure_workspace_dir(ws)?;
                    let _ = self.ensure_schema_files(Some(ws))?;
                }
            }
        }
        Ok(())
    }

    fn write_scoped_text(scope: ConfigScope, target_path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Self::scoped_io_error(scope, parent, source))?;
        }
        std::fs::write(target_path, content)
            .map_err(|source| Self::scoped_io_error(scope, target_path, source))
    }

    fn apply_and_write_scoped_patch(
        &self,
        scope: ConfigScope,
        path: &Path,
        patch: UserSettingsPatch,
    ) -> Result<()> {
        let format = ConfigFormat::from_path(path);
        let mut current: UserSettingsPatch = if path.is_file() {
            Self::read_scoped_file(scope, path).unwrap_or_default()
        } else {
            UserSettingsPatch::default()
        };
        if format == ConfigFormat::Json && current.schema.is_none() {
            current.schema = Some(DEFAULT_SCHEMA_RELATIVE_PATH.to_string());
        }
        current.merge_patch(patch);

        let serialized = Self::serialize_patch(&current, format)
            .map_err(|message| Self::scoped_parse_error(scope, path, message))?;
        Self::write_scoped_text(scope, path, &serialized)
    }

    fn serialize_patch(
        patch: &UserSettingsPatch,
        format: ConfigFormat,
    ) -> std::result::Result<String, String> {
        match format {
            ConfigFormat::Json => {
                let mut text =
                    serde_json::to_string_pretty(patch).map_err(|err| err.to_string())?;
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                Ok(text)
            }
            ConfigFormat::Yaml => {
                let mut patch_without_schema = patch.clone();
                patch_without_schema.schema = None;
                let body =
                    serde_yaml::to_string(&patch_without_schema).map_err(|err| err.to_string())?;
                let mut out = format!(
                    "# yaml-language-server: $schema={DEFAULT_SCHEMA_RELATIVE_PATH}\n{body}"
                );
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                Ok(out)
            }
        }
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
        Self::deserialize_by_format(path, &raw)
            .map_err(|message| Self::scoped_parse_error(scope, path, message))
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

    fn deserialize_by_format<T: DeserializeOwned>(
        path: &Path,
        raw: &str,
    ) -> std::result::Result<T, String> {
        match ConfigFormat::from_path(path) {
            ConfigFormat::Json => serde_json::from_str::<T>(raw).map_err(|err| err.to_string()),
            ConfigFormat::Yaml => serde_yaml::from_str::<T>(raw).map_err(|err| err.to_string()),
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
                    if lang_id == "json" || lang_id == "yaml" {
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
  },
  "mise": {
    "tools": {
      "fd": "latest"
    }
  }
}
"#,
                )
                .unwrap();
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.yaml"),
                    "lsp:\n  enabled: true\n  auto_install: true\n  request_timeout_ms: 9000\nmise:\n  tasks:\n    custom:\n      description: Custom\n      run: echo custom\n",
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
