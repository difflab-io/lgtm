use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lgtm_config::ConfigService;

use crate::error::{LgtmEnvError, Result};
use crate::models::mise::{GeneratedMiseFile, MiseTaskSummary};

const DEFAULT_MISE_INSTALLER_URL: &str = "https://mise.run";

/// Manages the `mise` binary lifecycle, `.lgtm/mise.toml` synthesis, and workspace task/tool execution.
#[derive(Debug, Clone)]
pub struct MiseService {
    config_service: ConfigService,
    search_system_paths: bool,
    installer_url: String,
}

impl Default for MiseService {
    fn default() -> Self {
        Self::new(ConfigService::new())
    }
}

impl MiseService {
    /// Creates a `MiseService` backed by `config_service` that searches `<lgtm_home>/bin/mise` and system paths.
    pub fn new(config_service: ConfigService) -> Self {
        Self {
            config_service,
            search_system_paths: true,
            installer_url: DEFAULT_MISE_INSTALLER_URL.to_string(),
        }
    }

    /// Creates an isolated `MiseService` that ignores global system `PATH` and installs only from `installer_url` into `<lgtm_home>/bin/mise`.
    pub fn with_isolated_env(config_service: ConfigService, installer_url: Option<String>) -> Self {
        Self {
            config_service,
            search_system_paths: false,
            installer_url: installer_url.unwrap_or_else(|| DEFAULT_MISE_INSTALLER_URL.to_string()),
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns the path where LGTM installs its managed `mise` binary (`<lgtm_home>/bin/mise`).
    pub fn get_managed_mise_binary_path(&self) -> PathBuf {
        self.config_service
            .get_lgtm_home_dir()
            .join("bin")
            .join(Self::binary_name())
    }

    /// Locates an existing usable `mise` binary in `<lgtm_home>/bin/mise` or standard system paths.
    pub fn find_mise_binary(&self) -> Option<PathBuf> {
        let managed = self.get_managed_mise_binary_path();
        if Self::is_usable_binary(&managed) {
            return Some(managed);
        }

        if !self.search_system_paths {
            return None;
        }

        if let Some(path_os) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_os) {
                let candidate = dir.join(Self::binary_name());
                if Self::is_usable_binary(&candidate) {
                    return Some(candidate);
                }
            }
        }

        Self::well_known_system_candidates()
            .into_iter()
            .find(|candidate| Self::is_usable_binary(candidate))
    }

    /// Returns `true` if a usable `mise` binary is currently installed.
    pub fn is_mise_installed(&self) -> bool {
        self.find_mise_binary().is_some()
    }

    /// Returns the path to a usable `mise` binary, installing it into `<lgtm_home>/bin/mise` if missing.
    pub fn ensure_mise_binary(&self) -> Result<PathBuf> {
        if let Some(existing) = self.find_mise_binary() {
            return Ok(existing);
        }
        self.install_mise_binary()
    }

    /// Downloads and installs the `mise` binary into `<lgtm_home>/bin/mise`.
    pub fn install_mise_binary(&self) -> Result<PathBuf> {
        let target_path = self.get_managed_mise_binary_path();
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                LgtmEnvError::MiseInstallFailed(format!(
                    "failed to create directory {}: {err}",
                    parent.display()
                ))
            })?;
        }

        let installer_url = self.resolve_installer_url();
        tracing::info!(
            target = %target_path.display(),
            installer_url = %installer_url,
            "installing managed mise binary"
        );

        let output = Self::run_installer(&installer_url, &target_path)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("installer exited with status {}", output.status)
            };
            return Err(LgtmEnvError::MiseInstallFailed(detail));
        }

        if !Self::is_usable_binary(&target_path) {
            return Err(LgtmEnvError::MiseInstallFailed(format!(
                "installed binary at {} is missing or not executable",
                target_path.display()
            )));
        }

        Ok(target_path)
    }

    /// Ensures `<workspace>/.lgtm/` and `.git/info/exclude` are initialized and writes `<workspace>/.lgtm/mise.toml`.
    pub fn sync_workspace_mise(&self, workspace_root: &Path) -> Result<PathBuf> {
        let _ = self.config_service.ensure_workspace_dir(workspace_root)?;
        let config = self.config_service.get_config(workspace_root)?;
        let detected_languages = ConfigService::detect_workspace_languages(&config, workspace_root);
        let generated = GeneratedMiseFile::from_config(&config, &detected_languages);
        let rendered = generated.render_toml()?;
        let mise_path = ConfigService::get_managed_mise_path(workspace_root);

        let needs_write = match std::fs::read_to_string(&mise_path) {
            Ok(existing) => existing != rendered,
            Err(_) => true,
        };

        if needs_write {
            std::fs::write(&mise_path, rendered).map_err(|source| LgtmEnvError::ManagedMiseIo {
                path: mise_path.clone(),
                source,
            })?;
        }

        Ok(mise_path)
    }

    /// Synchronizes `.lgtm/mise.toml` and returns all synthesized task summaries for the workspace.
    pub fn get_tasks(&self, workspace_root: &Path) -> Result<Vec<MiseTaskSummary>> {
        let _ = self.sync_workspace_mise(workspace_root)?;
        let config = self.config_service.get_config(workspace_root)?;
        let detected = ConfigService::detect_workspace_languages(&config, workspace_root);
        let generated = GeneratedMiseFile::from_config(&config, &detected);
        Ok(generated.get_task_summaries())
    }

    /// Synchronizes `.lgtm/mise.toml`, ensures `mise` is installed, and runs the named task via `mise run`.
    pub fn run_task(&self, workspace_root: &Path, task: &str, args: &[String]) -> Result<i32> {
        let mise_file = self.sync_workspace_mise(workspace_root)?;
        let config = self.config_service.get_config(workspace_root)?;
        let mise_bin = self.ensure_mise_binary()?;

        Self::trust_mise_file(&mise_bin, workspace_root, &mise_file);

        let mut cmd = Command::new(&mise_bin);
        cmd.current_dir(workspace_root)
            .env("MISE_CONFIG_FILE", &mise_file)
            .env("MISE_GLOBAL_CONFIG_FILE", &mise_file)
            .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if !config.lsp.auto_install {
            cmd.env("MISE_TASK_RUN_AUTO_INSTALL", "false")
                .env("MISE_NOT_FOUND_AUTO_INSTALL", "false");
        }

        cmd.arg("run");
        if !config.lsp.auto_install {
            cmd.arg("--skip-tools");
        }
        cmd.arg(task);
        if !args.is_empty() {
            cmd.args(args);
        }

        let status = cmd
            .status()
            .map_err(|err| LgtmEnvError::MiseFailed(err.to_string()))?;
        Ok(status.code().unwrap_or(1))
    }

    /// Ensures `mise` is installed and installs the requested `(tool, version)` pairs scoped to `.lgtm/mise.toml`.
    pub fn install_tools(&self, workspace_root: &Path, tools: &[(&str, &str)]) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }
        let mise_file = self.sync_workspace_mise(workspace_root)?;
        let mise_bin = self.ensure_mise_binary()?;
        Self::trust_mise_file(&mise_bin, workspace_root, &mise_file);

        let mut cmd = Command::new(&mise_bin);
        cmd.current_dir(workspace_root)
            .env("MISE_CONFIG_FILE", &mise_file)
            .env("MISE_GLOBAL_CONFIG_FILE", &mise_file)
            .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
            .arg("install");
        for (tool, version) in tools {
            cmd.arg(format!("{tool}@{version}"));
        }
        let output = cmd
            .output()
            .map_err(|err| LgtmEnvError::MiseFailed(err.to_string()))?;
        if !output.status.success() {
            return Err(LgtmEnvError::MiseFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }

    /// Builds a `mise exec` [`Command`] scoped to `<workspace>/.lgtm/mise.toml`, optionally installing `mise` if `auto_install_mise` is enabled.
    pub fn build_exec_command(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        tools: &BTreeMap<String, String>,
        program: &str,
        args: &[String],
        auto_install_mise: bool,
    ) -> Result<Command> {
        let mise_bin = if auto_install_mise {
            self.ensure_mise_binary()?
        } else {
            self.find_mise_binary()
                .ok_or(LgtmEnvError::MiseNotInstalled)?
        };
        let mise_file = ConfigService::get_managed_mise_path(workspace_root);
        let mut cmd = Command::new(mise_bin);
        cmd.arg("exec").arg("--cd").arg(project_root);
        for (tool, version) in tools {
            cmd.arg(format!("{tool}@{version}"));
        }
        cmd.arg("--").arg(program).args(args);
        cmd.env("MISE_CONFIG_FILE", &mise_file)
            .env("MISE_GLOBAL_CONFIG_FILE", &mise_file)
            .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
            .env("MISE_NOT_FOUND_AUTO_INSTALL", "false")
            .env("MISE_EXEC_AUTO_INSTALL", "false");
        Ok(cmd)
    }

    // Private -----------------------------------------------------------------

    fn binary_name() -> &'static str {
        if cfg!(windows) { "mise.exe" } else { "mise" }
    }

    fn well_known_system_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".local").join("bin").join(Self::binary_name()));
            candidates.push(
                home.join(".local")
                    .join("share")
                    .join("mise")
                    .join("bin")
                    .join(Self::binary_name()),
            );
            candidates.push(home.join("bin").join(Self::binary_name()));
        }
        #[cfg(not(windows))]
        {
            candidates.push(PathBuf::from("/opt/homebrew/bin/mise"));
            candidates.push(PathBuf::from("/usr/local/bin/mise"));
            candidates.push(PathBuf::from("/usr/bin/mise"));
        }
        candidates
    }

    fn is_usable_binary(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }
        Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn resolve_installer_url(&self) -> String {
        if let Ok(override_url) = std::env::var("LGTM_MISE_INSTALLER_URL") {
            let trimmed = override_url.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        self.installer_url.clone()
    }

    fn run_installer(installer_url: &str, target_path: &Path) -> Result<std::process::Output> {
        #[cfg(windows)]
        {
            Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "irm $env:LGTM_MISE_INSTALLER_URL | iex",
                ])
                .env("MISE_INSTALL_PATH", target_path)
                .env("LGTM_MISE_INSTALLER_URL", installer_url)
                .stdin(Stdio::null())
                .output()
                .map_err(|err| LgtmEnvError::MiseInstallFailed(err.to_string()))
        }
        #[cfg(not(windows))]
        {
            Command::new("sh")
                .args(["-c", "curl -fsSL \"$LGTM_MISE_INSTALLER_URL\" | sh"])
                .env("MISE_INSTALL_PATH", target_path)
                .env("LGTM_MISE_INSTALLER_URL", installer_url)
                .stdin(Stdio::null())
                .output()
                .map_err(|err| LgtmEnvError::MiseInstallFailed(err.to_string()))
        }
    }

    fn trust_mise_file(mise_bin: &Path, workspace_root: &Path, mise_file: &Path) {
        let _ = Command::new(mise_bin)
            .arg("trust")
            .arg(mise_file)
            .current_dir(workspace_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_workspace_with_rust_and_custom_tasks {
        use super::*;

        mod when_listing_managed_tasks {
            use super::*;

            #[test]
            fn should_sync_managed_mise_file_and_return_task_summaries() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = temp.path();
                let global_cfg = workspace_root.join("global.json");
                std::fs::write(
                    workspace_root.join("Cargo.toml"),
                    "[package]\nname = \"demo\"\n",
                )
                .unwrap();
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.json"),
                    "{\n  \"mise\": {\n    \"tasks\": {\n      \"greet\": {\n        \"description\": \"Say hello\",\n        \"run\": \"echo hello\"\n      }\n    }\n  }\n}\n",
                )
                .unwrap();
                let service = MiseService::new(ConfigService::with_global_config_path(global_cfg));

                // Act
                let tasks = service.get_tasks(workspace_root).unwrap();

                // Assert
                assert!(workspace_root.join(".lgtm/mise.toml").is_file());
                let names: Vec<String> = tasks.into_iter().map(|t| t.name).collect();
                assert!(names.contains(&"check".to_string()));
                assert!(names.contains(&"rust:check".to_string()));
                assert!(names.contains(&"greet".to_string()));
            }
        }
    }
}
