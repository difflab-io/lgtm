use clap::{Args, Subcommand};

use crate::{
    backend::{
        extensions::git::get_cwd_git_dir,
        proto::lgtm::v1::{
            CloseReviewRequest, CreateReviewRequest, ListReviewsRequest, RefreshReviewRequest,
            ReviewGroupBy as ProtoGroupBy, ReviewOutcome as ProtoOutcome, ShowReviewRequest,
        },
    },
    models::{
        ActorArgs, ListReviewsCommandOutput, LocalArgs, RefreshReviewCommandOutput, ReviewGroupBy,
        ReviewOutcome, ReviewSessionCommandOutput, ShowReviewCommandOutput, parse_nonempty,
    },
    rpc_client,
    store::Store,
};

#[derive(Subcommand)]
pub enum ReviewCommand {
    /// List open or all reviews in the repository.
    List {
        #[command(flatten)]
        location: LocalArgs,
        #[arg(long)]
        all: bool,
    },
    /// Create a local snapshot review or synchronize a remote pull request review.
    New {
        #[command(flatten)]
        location: LocalArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Refresh review comments, remote pull request state, and line anchors.
    Refresh {
        #[arg(value_parser = parse_nonempty)]
        review: Option<String>,
        #[command(flatten)]
        location: LocalArgs,
    },
    /// Show review comments and inline unified diff hunks.
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
    /// Publish/close the active draft review session with an outcome and optional summary body.
    Close {
        #[arg(value_parser = parse_nonempty)]
        review: Option<String>,
        #[command(flatten)]
        location: LocalArgs,
        #[arg(long = "with", value_enum)]
        outcome: ReviewOutcome,
        #[arg(long)]
        body: Option<String>,
        #[command(flatten)]
        actor: ActorArgs,
    },
}

#[derive(Args)]
pub struct ReviewArgs {
    #[command(subcommand)]
    pub command: ReviewCommand,
}

pub async fn run(args: ReviewArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let git_dir = get_cwd_git_dir()?;
    let client = rpc_client::connect(store).await?;

    match args.command {
        ReviewCommand::List { location, all } => {
            let response = client
                .list_reviews(ListReviewsRequest {
                    git_dir,
                    local: location.local,
                    all,
                    ..Default::default()
                })
                .await?
                .into_owned();
            ListReviewsCommandOutput::from(response).print(json);
            Ok(())
        }
        ReviewCommand::New { location, actor } => {
            let response = client
                .create_review(CreateReviewRequest {
                    git_dir,
                    local: location.local,
                    as_agent: actor.agent_name,
                    ..Default::default()
                })
                .await?
                .into_owned();
            ReviewSessionCommandOutput::try_from(response)?.print_created(json);
            Ok(())
        }
        ReviewCommand::Refresh { review, location } => {
            let response = client
                .refresh_review(RefreshReviewRequest {
                    git_dir,
                    selector: review,
                    local: location.local,
                    ..Default::default()
                })
                .await?
                .into_owned();
            RefreshReviewCommandOutput::try_from(response)?.print(json);
            Ok(())
        }
        ReviewCommand::Show {
            review,
            location,
            group_by,
            unresolved,
        } => {
            let proto_group: ProtoGroupBy = group_by.into();
            let response = client
                .show_review(ShowReviewRequest {
                    git_dir,
                    selector: review,
                    local: location.local,
                    group_by: proto_group.into(),
                    unresolved,
                    ..Default::default()
                })
                .await?
                .into_owned();
            ShowReviewCommandOutput::try_from(response)?.print(group_by, unresolved, json);
            Ok(())
        }
        ReviewCommand::Close {
            review,
            location,
            outcome,
            body,
            actor,
        } => {
            let proto_outcome: ProtoOutcome = outcome.into();
            let response = client
                .close_review(CloseReviewRequest {
                    git_dir,
                    selector: review,
                    local: location.local,
                    with_outcome: proto_outcome.into(),
                    body,
                    as_agent: actor.agent_name,
                    ..Default::default()
                })
                .await?
                .into_owned();
            ReviewSessionCommandOutput::try_from(response)?.print_closed(outcome, json);
            Ok(())
        }
    }
}
