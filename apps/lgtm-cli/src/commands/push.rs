use clap::Args;
use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::PushRequest,
    commands::resolve_git_dir,
    models::{LocalArgs, short_sha},
    rpc_client::Client,
    store::Store,
};

#[derive(Args)]
pub struct PushArgs {
    #[command(flatten)]
    pub location: LocalArgs,
}

pub async fn run(args: PushArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = resolve_git_dir()?;
    let client = Client::connect(store).await?;
    let response = client
        .push(PushRequest {
            git_dir,
            local: args.location.local,
            ..Default::default()
        })
        .await?;

    if json {
        println!(
            "{}",
            json!({
                "branch": response.branch,
                "head_sha": response.head_sha,
                "repository_url": response.repository_url,
                "local": response.local,
            })
        );
    } else {
        println!(
            "Pushed {} ({}) to {}",
            response.branch,
            short_sha(&response.head_sha),
            response.repository_url
        );
    }
    Ok(())
}
