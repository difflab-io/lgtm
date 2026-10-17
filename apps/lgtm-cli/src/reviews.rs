//! Review command scaffold. Persistence and domain workflows belong to backend RPCs.
use crate::{
    context::{ActorArgs, ContextArgs},
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use std::path::PathBuf;

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
#[derive(Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}
#[derive(Subcommand)]
pub enum SessionCommand {
    /// Scaffold: start or resume the context's one active review.
    Start {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Scaffold: show active/latest review metadata, or a historical session.
    Show {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
    },
    /// Scaffold: publish drafts locally and complete the review (no forge write).
    Complete {
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
    /// Scaffold: export current conversation text with selected review metadata.
    Export {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Scaffold: refresh PR state through the future backend RPC.
    Refresh {
        #[command(flatten)]
        context: ContextArgs,
    },
    /// Scaffold: submit through the future backend RPC.
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
#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: SessionsCommand,
}
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
#[derive(Args)]
pub struct CommentsArgs {
    #[command(subcommand)]
    pub command: CommentsCommand,
}
#[derive(Subcommand)]
pub enum CommentsCommand {
    List {
        #[command(flatten)]
        context: ContextArgs,
    },
    Add {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long, requires = "file")]
        line: Option<i64>,
        #[arg(long, requires = "line")]
        end_line: Option<i64>,
        #[arg(long,requires="line",value_parser=["old","new"])]
        side: Option<String>,
    },
    Reply {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
    },
    Resolve {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    Reopen {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
}

/// Start/reuse the UI/backend daemon and open the current path without touching review data.
pub async fn review(args: ReviewArgs, store: &Store) -> Result<()> {
    let path = args
        .context
        .notes
        .or(args.context.path)
        .unwrap_or(std::env::current_dir()?);
    let path = path
        .canonicalize()
        .context("Review path must exist and be accessible")?;
    crate::ui::open_review(&path, None, None, args.port, !args.no_open, store).await
}

/// Keep the command shape available until review RPCs exist.
pub async fn session(_args: SessionArgs, _store: &Store) -> Result<()> {
    bail!("Session commands are scaffolded; backend review RPCs are not implemented")
}

/// Keep the command shape available until review RPCs exist.
pub async fn sessions(_args: SessionsArgs, _store: &Store) -> Result<()> {
    bail!("Session listing is scaffolded; backend review RPCs are not implemented")
}

/// Keep the command shape available until review RPCs exist.
pub async fn comments(_args: CommentsArgs, _store: &Store) -> Result<()> {
    bail!("Comment commands are scaffolded; backend review RPCs are not implemented")
}
