//! Session command arguments and RPC callbacks.
use super::{
    client_for_actor, get_conversation, get_review, get_reviews, get_scope, key, print_result,
    start,
};
use crate::{
    models::{ActorArgs, ContextArgs, MutationArgs},
    rpc_client::Client,
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;

// Types -----------------------------------------------------------------------

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List saved review attempts, active by default.
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
    /// Start or resume the matching review comparison.
    Start {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Show the active/latest review, or an explicit historical review.
    Show {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
    },
    /// Publish local drafts and complete the review without a forge write.
    Complete {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        request_key: Option<String>,
    },
    /// Export concise Markdown feedback without changing review state.
    Export {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Capture the latest saved source as another revision in this review.
    Refresh {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        mutation: MutationArgs,
    },
    /// Reconcile a frozen submission, completing only a verified pending review.
    Reconcile {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
    },
    /// Submit through the backend RPC.
    Submit {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        outcome: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        request_key: Option<String>,
    },
}

// Command ---------------------------------------------------------------------

#[derive(Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

// Callbacks -------------------------------------------------------------------

pub async fn run(args: SessionArgs, store: &Store, json_output: bool) -> Result<()> {
    match args.command {
        SessionCommand::List {
            path,
            repo,
            all,
            completed,
        } => {
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
                reviews.retain(|review| {
                    review["target"]["pullRequest"]["repositoryUrl"] == repository
                });
            }
            print_result(&json!({"reviews":reviews}), json_output)
        }
        SessionCommand::Start { context, actor } => {
            let client = client_for_actor(store, &actor).await?;
            print_result(&start(&client, &context).await?, json_output)
        }
        SessionCommand::Show { context, session } => {
            let client = Client::new(store)?;
            let review = match session {
                Some(id) => {
                    client.call("GetReview", json!({"reviewId":id})).await?["review"].clone()
                }
                None => get_review(&client, &context).await?,
            };
            print_result(&json!({"review":review}), json_output)
        }
        SessionCommand::Complete {
            context,
            actor,
            body,
            request_key,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let review = get_review(&client, &context).await?;
            print_result(&client.call("CompleteReview", json!({"reviewId":review["id"], "expectedVersion":review["version"],"body":body.unwrap_or_default(),"requestKey":key(request_key)})).await?,json_output)
        }
        SessionCommand::Export {
            context,
            session,
            output,
        } => {
            let client = Client::new(store)?;
            let scope = match session {
                Some(id) => json!({"reviewId":id}),
                None => get_scope(&client, &context, None, false).await?,
            };
            let result = client.call("Export", json!({"scope":scope})).await?;
            let text = result["markdown"]
                .as_str()
                .context("Backend returned no export content")?;
            if let Some(path) = output {
                std::fs::write(&path, text)?;
                if json_output {
                    print_result(&json!({"output":path,"markdown":text}), true)?;
                }
            } else if json_output {
                print_result(&result, true)?;
            } else {
                print!("{text}");
            }
            Ok(())
        }
        SessionCommand::Refresh { context, mutation } => {
            let client = client_for_actor(store, &ActorArgs::default()).await?;
            let scope = get_scope(&client, &context, None, false).await?;
            let version = match mutation.expected_version {
                Some(version) => json!(version),
                None => get_conversation(&client, &scope).await?["version"].clone(),
            };
            print_result(&client.call("RefreshReview",json!({"scope":scope,"expectedVersion":version,"requestKey":key(mutation.request_key)})).await?,json_output)
        }
        SessionCommand::Reconcile {
            context,
            actor,
            session,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let id = match session {
                Some(id) => json!(id),
                None => get_review(&client, &context).await?["id"].clone(),
            };
            print_result(
                &client
                    .call("ReconcileSubmission", json!({"reviewId":id}))
                    .await?,
                json_output,
            )
        }
        SessionCommand::Submit {
            context,
            actor,
            outcome,
            body,
            request_key,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let review = get_review(&client, &context).await?;
            let comparison = &review["comparison"]["pullRequest"];
            if comparison.is_null() {
                bail!("Local reviews cannot be submitted to GitHub; use session complete");
            }
            print_result(&client.call("SubmitReview",json!({"reviewId":review["id"],"outcome":outcome.unwrap_or_else(||"comment".into()),"body":body.unwrap_or_default(),"requestKey":key(request_key),"expectedHead":comparison["headCommit"],"expectedBase":comparison["baseCommit"],"expectedVersion":review["version"]})).await?,json_output)
        }
    }
}
