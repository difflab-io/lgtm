use clap::Args;

use crate::{
    backend::{config::ConfigService, env::MiseService},
    models::RunCommandOutput,
};

/// Arguments for the `lgtm run` command.
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

/// Lists or executes tasks configured in the workspace's LGTM-managed `.lgtm/mise.toml`.
#[tracing::instrument(skip_all, fields(list = args.list, task = ?args.task))]
pub async fn run(args: RunArgs, json: bool) -> anyhow::Result<()> {
    let workspace_root = std::env::current_dir()?;
    let config_service = ConfigService::new();
    let mise_path = ConfigService::get_managed_mise_path(&workspace_root);
    let mise_file = mise_path.to_string_lossy().to_string();
    let mise_service = MiseService::new(config_service);

    let Some(task_name) = args.task.filter(|_| !args.list) else {
        let tasks = mise_service.get_tasks(&workspace_root)?;
        RunCommandOutput::Tasks { mise_file, tasks }.print(json);
        return Ok(());
    };

    let exit_code = mise_service.run_task(&workspace_root, &task_name, &args.args)?;
    let output = RunCommandOutput::Executed {
        mise_file,
        task: Some(task_name.clone()),
        exit_code,
    };
    output.print(json);

    if exit_code != 0 {
        anyhow::bail!("task '{task_name}' exited with status {exit_code}");
    }

    Ok(())
}
