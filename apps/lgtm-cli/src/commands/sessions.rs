//! Sessions command arguments and RPC callbacks.
use super::{get_reviews, print_result};
use crate::{rpc_client::Client, store::Store};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;

// Types -----------------------------------------------------------------------

#[derive(Subcommand)]
pub enum SessionsCommand {
    List {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        completed: bool,
    },
}

// Command ---------------------------------------------------------------------

#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: SessionsCommand,
}

// Callbacks -------------------------------------------------------------------

pub async fn run(args: SessionsArgs, store: &Store, json_output: bool) -> Result<()> {
    let SessionsCommand::List {
        path,
        repo,
        all,
        completed,
    } = args.command;
    let client = Client::new(store)?;
    let mut reviews = get_reviews(
        &client,
        None,
        if all {
            None
        } else if completed {
            Some("completed")
        } else {
            Some("active")
        },
    )
    .await?;
    if let Some(path) = path {
        let path = path.canonicalize()?.to_string_lossy().into_owned();
        reviews.retain(|review| {
            review["target"]["localGit"]["path"] == path
                || review["target"]["directory"]["path"] == path
        });
    }
    if let Some(repository) = repo {
        reviews.retain(|review| review["target"]["pullRequest"]["repositoryUrl"] == repository);
    }
    print_result(&json!({"reviews":reviews}), json_output)
}
