use crate::{
    commands::NotImplemented,
    models::{ActorArgs, LocalArgs},
};
use clap::Args;

#[derive(Args)]
pub struct CommitArgs {
    #[command(flatten)]
    pub actor: ActorArgs,
    #[command(flatten)]
    pub location: LocalArgs,
    #[arg(short, long)]
    pub message: Option<String>,
}

pub fn run(_args: CommitArgs) -> anyhow::Result<()> {
    Err(NotImplemented {
        operation: "commit",
    }
    .into())
}
