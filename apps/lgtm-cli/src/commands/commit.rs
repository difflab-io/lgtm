use clap::Args;
use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::CommitRequest,
    commands::resolve_git_dir,
    models::{LocalArgs, format_identity_display, format_identity_json, short_sha},
    rpc_client::Client,
    store::Store,
};

#[derive(Args)]
pub struct CommitArgs {
    #[command(flatten)]
    pub location: LocalArgs,
    #[arg(short, long)]
    pub message: Option<String>,
}

pub async fn run(args: CommitArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = resolve_git_dir()?;
    let client = Client::connect(store).await?;
    let response = client
        .commit(CommitRequest {
            git_dir,
            message: args.message,
            local: args.location.local,
            ..Default::default()
        })
        .await?;

    if json {
        println!(
            "{}",
            json!({
                "commit_sha": response.commit_sha,
                "branch": response.branch,
                "author": format_identity_json(response.author.as_option()),
                "local": response.local,
            })
        );
    } else {
        let mode = if response.local {
            "local snapshot"
        } else {
            "working tree"
        };
        println!(
            "Committed {} on {} ({mode}) by {}",
            short_sha(&response.commit_sha),
            response.branch,
            format_identity_display(response.author.as_option())
        );
    }
    Ok(())
}
