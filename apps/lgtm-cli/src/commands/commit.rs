use clap::Args;

use crate::{
    backend::{extensions::git::get_cwd_git_dir, proto::lgtm::v1::CommitRequest},
    models::{CommitView, LocalArgs},
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

    CommitView::from(response).print(json);
    Ok(())
}
