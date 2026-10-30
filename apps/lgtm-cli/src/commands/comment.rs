use crate::{
    commands::NotImplemented,
    models::{ActorArgs, LocalArgs, parse_nonempty},
};
use clap::Args;

#[derive(Args)]
pub struct CommentArgs {
    #[arg(value_parser = parse_nonempty)]
    pub body: String,
    #[arg(long, value_parser = parse_nonempty)]
    pub review: Option<String>,
    #[command(flatten)]
    pub location: LocalArgs,
    #[command(flatten)]
    pub actor: ActorArgs,
}

pub fn run(_args: CommentArgs) -> anyhow::Result<()> {
    Err(NotImplemented {
        operation: "comment",
    }
    .into())
}
