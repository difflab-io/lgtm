use clap::{Args, Subcommand};

use crate::{
    backend::{
        extensions::git::get_cwd_git_dir,
        proto::lgtm::v1::{CreateCommentRequest, DeleteCommentRequest, UpdateCommentRequest},
    },
    models::{
        ActorArgs, CommentView, DeletedCommentView, LocalArgs, parse_comment_id, parse_nonempty,
    },
    rpc_client,
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
    let git_dir = get_cwd_git_dir()?;
    let client = rpc_client::connect(store).await?;

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
                .await?
                .into_owned();
            CommentView::try_from(response)?.print_created(json);
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
                .await?
                .into_owned();
            CommentView::try_from(response)?.print_updated(json);
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
                .await?
                .into_owned();
            DeletedCommentView::from(response).print(json);
            Ok(())
        }
    }
}
