use clap::Args;
use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::AddRequest,
    commands::resolve_git_dir,
    models::{LocalArgs, parse_nonempty},
    rpc_client::Client,
    store::Store,
};

#[derive(Args)]
pub struct AddArgs {
    #[arg(required = true, num_args = 1.., value_parser = parse_nonempty)]
    pub paths: Vec<String>,
    #[command(flatten)]
    pub location: LocalArgs,
}

pub async fn run(args: AddArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = resolve_git_dir()?;
    let client = Client::connect(store).await?;
    let response = client
        .add(AddRequest {
            git_dir,
            paths: args.paths,
            local: args.location.local,
            ..Default::default()
        })
        .await?;

    if json {
        println!(
            "{}",
            json!({
                "staged_paths": response.staged_paths,
                "local": args.location.local,
            })
        );
    } else {
        let mode = if args.location.local {
            "local snapshot"
        } else {
            "working tree"
        };
        println!(
            "Staged {} path(s) ({mode}): {}",
            response.staged_paths.len(),
            response.staged_paths.join(", ")
        );
    }
    Ok(())
}
