use clap::Args;
use lgtm_forge::{extensions::git::get_cwd_git_dir, proto::lgtm::v1::CommitRequest};

use crate::{
    models::{CommitCommandOutput, LocalArgs},
    rpc_client,
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
    let git_dir = get_cwd_git_dir()?;
    let client = rpc_client::connect(store).await?;
    let response = client
        .commit(CommitRequest {
            git_dir,
            message: args.message,
            local: args.location.local,
            ..Default::default()
        })
        .await?
        .into_owned();

    CommitCommandOutput::from(response).print(json);
    Ok(())
}
