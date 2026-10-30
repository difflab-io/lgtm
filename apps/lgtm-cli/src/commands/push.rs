use crate::{commands::NotImplemented, models::LocalArgs};
use clap::Args;

#[derive(Args)]
pub struct PushArgs {
    #[command(flatten)]
    pub location: LocalArgs,
}

pub fn run(_args: PushArgs) -> anyhow::Result<()> {
    Err(NotImplemented { operation: "push" }.into())
}
