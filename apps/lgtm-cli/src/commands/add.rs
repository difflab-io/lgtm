use clap::Args;

use crate::{
    backend::{extensions::git::get_cwd_git_dir, proto::lgtm::v1::AddRequest},
    models::{AddCommandOutput, LocalArgs, parse_nonempty},
    rpc_client,
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
    let git_dir = get_cwd_git_dir()?;
    let client = rpc_client::connect(store).await?;
    let local = args.location.local;
    let response = client
        .add(AddRequest {
            git_dir,
            paths: args.paths,
            local,
            ..Default::default()
        })
        .await?
        .into_owned();

    AddCommandOutput::from((response, local)).print(json);
    Ok(())
}
