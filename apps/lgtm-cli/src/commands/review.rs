use crate::{
    commands::NotImplemented,
    models::{LocalArgs, ReviewOutcome, parse_nonempty},
    rpc_client::Client,
    store::Store,
};
use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum ReviewCommand {
    /// Call the backend hello-review demonstration; no reviews are persisted.
    List(LocalArgs),
    /// Create a review (not implemented yet).
    New(LocalArgs),
    /// Close a review (not implemented yet).
    Close {
        #[arg(value_parser = parse_nonempty)]
        review_id: String,
        #[arg(long = "with", value_enum)]
        outcome: ReviewOutcome,
        #[arg(long)]
        body: Option<String>,
    },
}

#[derive(Args)]
pub struct ReviewArgs {
    #[command(subcommand)]
    pub command: ReviewCommand,
}

pub async fn run(args: ReviewArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    match args.command {
        ReviewCommand::List(_) => {
            let response = Client::new(store)?.get_hello("CLI").await?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({"demo": true, "message": response.message})
                );
            } else {
                println!("Demo review: {} (no saved reviews)", response.message);
            }
            Ok(())
        }
        ReviewCommand::New(_) => Err(NotImplemented {
            operation: "review new",
        }
        .into()),
        ReviewCommand::Close { .. } => Err(NotImplemented {
            operation: "review close",
        }
        .into()),
    }
}
