use crate::{
    commands::NotImplemented,
    models::{LocalArgs, ReviewGroupBy, ReviewOutcome, parse_nonempty},
    rpc_client::Client,
    store::Store,
};
use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum ReviewCommand {
    /// Call the backend hello-review demonstration; no reviews are persisted.
    List {
        #[command(flatten)]
        location: LocalArgs,
        #[arg(long)]
        all: bool,
    },
    /// Create a review (not implemented yet).
    New(LocalArgs),
    /// Refresh review comments and anchors (not implemented yet).
    Refresh {
        #[arg(value_parser = parse_nonempty)]
        review: Option<String>,
        #[command(flatten)]
        location: LocalArgs,
    },
    /// Show review comments and diff hunks (not implemented yet).
    Show {
        #[arg(value_parser = parse_nonempty)]
        review: Option<String>,
        #[command(flatten)]
        location: LocalArgs,
        #[arg(long, value_enum, default_value_t = ReviewGroupBy::File)]
        group_by: ReviewGroupBy,
        #[arg(long)]
        unresolved: bool,
    },
    /// Close a review (not implemented yet).
    Close {
        #[arg(value_parser = parse_nonempty)]
        review: Option<String>,
        #[command(flatten)]
        location: LocalArgs,
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
        ReviewCommand::List { .. } => {
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
        ReviewCommand::Refresh { .. } => Err(NotImplemented {
            operation: "review refresh",
        }
        .into()),
        ReviewCommand::Show { .. } => Err(NotImplemented {
            operation: "review show",
        }
        .into()),
        ReviewCommand::Close { .. } => Err(NotImplemented {
            operation: "review close",
        }
        .into()),
    }
}
