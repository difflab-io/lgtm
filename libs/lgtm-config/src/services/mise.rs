use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{ConfigError, Result};
use crate::models::mise::{GeneratedMiseFile, MiseTaskSummary};
use crate::services::config::ConfigService;

#[derive(Debug, Clone, Default)]
pub struct MiseService {
    config_service: ConfigService,
}

impl MiseService {
    pub fn new(config_service: ConfigService) -> Self {
        Self { config_service }
    }

    // Public ------------------------------------------------------------------

    pub fn sync_workspace(&self, workspace_root: &Path) -> Result<PathBuf> {
        self.config_service.ensure_workspace_layout(workspace_root)
    }

    pub fn get_tasks(&self, workspace_root: &Path) -> Result<Vec<MiseTaskSummary>> {
        let _ = self.sync_workspace(workspace_root)?;
        let config = self.config_service.get_config(workspace_root)?;
        let detected = config.detect_workspace_languages(workspace_root);
        let generated = GeneratedMiseFile::from_config(&config, &detected);
        Ok(generated.task_summaries())
    }

    pub fn run_task(
        &self,
        workspace_root: &Path,
        task: Option<&str>,
        args: &[String],
    ) -> Result<i32> {
        let mise_file = self.sync_workspace(workspace_root)?;
        let config = self.config_service.get_config(workspace_root)?;

        let _ = Command::new("mise")
            .arg("trust")
            .arg(&mise_file)
            .current_dir(workspace_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let mut cmd = Command::new("mise");
        cmd.current_dir(workspace_root)
            .env("MISE_CONFIG_FILE", &mise_file)
            .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if !config.lsp.auto_install {
            cmd.env("MISE_TASK_RUN_AUTO_INSTALL", "false")
                .env("MISE_NOT_FOUND_AUTO_INSTALL", "false");
        }

        if let Some(task_name) = task {
            cmd.arg("run");
            if !config.lsp.auto_install {
                cmd.arg("--skip-tools");
            }
            cmd.arg(task_name);
            if !args.is_empty() {
                cmd.args(args);
            }
        } else {
            cmd.arg("tasks").arg("ls");
        }

        let status = cmd
            .status()
            .map_err(|err| ConfigError::MiseFailed(err.to_string()))?;
        Ok(status.code().unwrap_or(1))
    }

    pub fn install_tools(&self, workspace_root: &Path, tools: &[(&str, &str)]) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }
        let mise_file = self.sync_workspace(workspace_root)?;
        let mut cmd = Command::new("mise");
        cmd.current_dir(workspace_root)
            .env("MISE_CONFIG_FILE", &mise_file)
            .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
            .arg("install");
        for (tool, version) in tools {
            cmd.arg(format!("{tool}@{version}"));
        }
        let output = cmd
            .output()
            .map_err(|err| ConfigError::MiseFailed(err.to_string()))?;
        if !output.status.success() {
            return Err(ConfigError::MiseFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(())
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
                let global_cfg = workspace_root.join("global.toml");
                std::fs::write(
                    workspace_root.join("Cargo.toml"),
                    "[package]\nname = \"demo\"\n",
                )
                .unwrap();
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.toml"),
                    "[mise.tasks.greet]\ndescription = \"Say hello\"\nrun = \"echo hello\"\n",
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
