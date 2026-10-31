use anyhow::Context;
use clap::{Args, Subcommand};
use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::{CreateCommentRequest, DeleteCommentRequest, UpdateCommentRequest},
    commands::resolve_git_dir,
    models::{
        ActorArgs, LocalArgs, format_comment_json, format_identity_display, parse_comment_id,
        parse_nonempty,
    },
    rpc_client::Client,
    store::Store,
};

#[derive(Subcommand)]
pub enum CommentCommand {
    /// Create a draft inline, multi-line range, general, or reply comment.
    New {
        #[arg(value_parser = parse_nonempty)]
        body: String,
        #[arg(long, value_parser = parse_nonempty)]
        review: Option<String>,
        #[arg(long, value_parser = parse_nonempty)]
        file: Option<String>,
        #[arg(long)]
        start_line: Option<u32>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, value_parser = parse_comment_id)]
        reply_to: Option<u64>,
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Update a comment's body, line anchor, or thread resolution status.
    Update {
        #[arg(value_parser = parse_comment_id)]
        comment_id: u64,
        #[arg(long, value_parser = parse_nonempty)]
        body: Option<String>,
        #[arg(long, value_parser = parse_nonempty)]
        file: Option<String>,
        #[arg(long)]
        start_line: Option<u32>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long)]
        resolved: Option<bool>,
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Delete a comment by its numeric ID.
    Delete {
        #[arg(value_parser = parse_comment_id)]
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

pub async fn run(args: CommentArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = resolve_git_dir()?;
    let client = Client::connect(store).await?;

    match args.command {
        CommentCommand::New {
            body,
            review,
            file,
            start_line,
            line,
            reply_to,
            location,
            actor,
        } => {
            let response = client
                .create_comment(CreateCommentRequest {
                    git_dir,
                    review_selector: review,
                    body,
                    file_path: file,
                    start_line,
                    line,
                    reply_to,
                    local: location.local,
                    as_agent: actor.agent_name,
                    ..Default::default()
                })
                .await?;
            let comment = response
                .comment
                .as_option()
                .context("CreateComment response missing comment")?;
            if json {
                println!("{}", json!({ "comment": format_comment_json(comment) }));
            } else {
                println!(
                    "Created comment [{}] by {}",
                    comment.number,
                    format_identity_display(comment.author.as_option()),
                );
            }
            Ok(())
        }
        CommentCommand::Update {
            comment_id,
            body,
            file,
            start_line,
            line,
            resolved,
            location,
            actor,
        } => {
            let response = client
                .update_comment(UpdateCommentRequest {
                    git_dir,
                    comment_number: comment_id,
                    body,
                    file_path: file,
                    start_line,
                    line,
                    resolved,
                    local: location.local,
                    as_agent: actor.agent_name,
                    ..Default::default()
                })
                .await?;
            let comment = response
                .comment
                .as_option()
                .context("UpdateComment response missing comment")?;
            if json {
                println!("{}", json!({ "comment": format_comment_json(comment) }));
            } else {
                println!("Updated comment [{}]", comment.number);
            }
            Ok(())
        }
        CommentCommand::Delete {
            comment_id,
            location,
            actor,
        } => {
            let response = client
                .delete_comment(DeleteCommentRequest {
                    git_dir,
                    comment_number: comment_id,
                    local: location.local,
                    as_agent: actor.agent_name,
                    ..Default::default()
                })
                .await?;
            if json {
                println!("{}", json!({ "deleted_number": response.deleted_number }));
            } else {
                println!("Deleted comment [{}]", response.deleted_number);
            }
            Ok(())
        }
    }
}
