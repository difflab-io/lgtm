use clap::Args;
use lgtm_config::{ConfigService, MiseService};

use crate::{lgtm_config, models::RunCommandOutput};

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// List tasks defined in the LGTM-managed `.lgtm/mise.toml`.
    #[arg(long, short = 'l')]
    pub list: bool,
    /// Task name to execute via `mise run`.
    pub task: Option<String>,
    /// Additional arguments forwarded to `mise run <task>`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[tracing::instrument(skip_all, fields(list = args.list, task = ?args.task))]
pub async fn run(args: RunArgs, json: bool) -> anyhow::Result<()> {
    let workspace_root = std::env::current_dir()?;
    let config_service = ConfigService::new();
    let mise_service = MiseService::new(config_service);
    let mise_path = mise_service.sync_workspace(&workspace_root)?;
    let mise_file = mise_path.to_string_lossy().to_string();

    if args.list || args.task.is_none() {
        let tasks = mise_service.get_tasks(&workspace_root)?;
        RunCommandOutput::Tasks { mise_file, tasks }.print(json);
        return Ok(());
    }

    let task_name = args.task.clone();
    let exit_code = mise_service.run_task(&workspace_root, task_name.as_deref(), &args.args)?;
    let output = RunCommandOutput::Executed {
        mise_file,
        task: task_name.clone(),
        exit_code,
    };
    output.print(json);

    if exit_code != 0 {
        anyhow::bail!(
            "task '{}' exited with status {exit_code}",
            task_name.unwrap_or_default()
        );
    }

    Ok(())
}
