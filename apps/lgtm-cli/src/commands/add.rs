use crate::{
    commands::NotImplemented,
    models::{LocalArgs, parse_nonempty},
};
use clap::Args;

#[derive(Args)]
pub struct AddArgs {
    #[arg(required = true, num_args = 1.., value_parser = parse_nonempty)]
    pub paths: Vec<String>,
    #[command(flatten)]
    pub location: LocalArgs,
}

pub fn run(_args: AddArgs) -> anyhow::Result<()> {
    Err(NotImplemented { operation: "add" }.into())
}
