use clap::Args;
use lgtm_forge::{extensions::git::get_cwd_git_dir, proto::lgtm::v1::PushRequest};

use crate::{
    models::{LocalArgs, PushCommandOutput},
    rpc_client,
    store::Store,
};

#[derive(Args)]
pub struct PushArgs {
    #[command(flatten)]
    pub location: LocalArgs,
}

pub async fn run(args: PushArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = get_cwd_git_dir()?;
    let client = rpc_client::connect(store).await?;
    let response = client
        .push(PushRequest {
            git_dir,
            local: args.location.local,
            ..Default::default()
        })
        .await?
        .into_owned();

    PushCommandOutput::from(response).print(json);
    Ok(())
}
