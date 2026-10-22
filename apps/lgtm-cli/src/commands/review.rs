//! Review command arguments and RPC callbacks.
use super::{client_for_actor, location, print_result, start};
use crate::{
    models::{ActorArgs, ContextArgs},
    store::Store,
};
use anyhow::Result;
use clap::Args;

// Command ---------------------------------------------------------------------

#[derive(Args)]
pub struct ReviewArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[command(flatten)]
    pub actor: ActorArgs,
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    #[arg(long)]
    pub no_open: bool,
}

// Callbacks -------------------------------------------------------------------

/// Machine mode starts a review through the same public contract used by editor clients.
pub async fn run(args: ReviewArgs, store: &Store, json_output: bool) -> Result<()> {
    if !json_output {
        let path = location(&args.context)?;
        return crate::ui::open_review(&path, None, None, args.port, !args.no_open, store).await;
    }
    let client = client_for_actor(store, &args.actor).await?;
    print_result(&start(&client, &args.context).await?, true)
}
