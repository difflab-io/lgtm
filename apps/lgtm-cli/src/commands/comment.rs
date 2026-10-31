use crate::{
    commands::NotImplemented,
    models::{ActorArgs, LocalArgs, parse_nonempty},
};
use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum CommentCommand {
    /// Create a comment (not implemented yet).
    New {
        #[arg(value_parser = parse_nonempty)]
        body: String,
        #[arg(long, value_parser = parse_nonempty)]
        review: Option<String>,
        #[arg(long, value_parser = parse_nonempty)]
        file: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long)]
        reply_to: Option<u64>,
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Update a comment body or anchor (not implemented yet).
    Update {
        comment_id: u64,
        #[arg(long, value_parser = parse_nonempty)]
        body: Option<String>,
        #[arg(long, value_parser = parse_nonempty)]
        file: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Delete a comment (not implemented yet).
    Delete {
        comment_id: u64,
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
}

#[derive(Args)]
pub struct CommentArgs {
    #[command(subcommand)]
    pub command: CommentCommand,
}

pub fn run(args: CommentArgs) -> anyhow::Result<()> {
    let operation = match args.command {
        CommentCommand::New { .. } => "comment new",
        CommentCommand::Update { .. } => "comment update",
        CommentCommand::Delete { .. } => "comment delete",
    };
    Err(NotImplemented { operation }.into())
}
