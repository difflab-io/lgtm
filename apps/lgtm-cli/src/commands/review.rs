use std::collections::HashMap;

use anyhow::Context;
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::{
    backend::proto::lgtm::v1::{
        CloseReviewRequest, CommentAnchorDetail, CommentDetail, CreateReviewRequest,
        DiffHunkDetail, ListReviewsRequest, RefreshReviewRequest, ReviewGroupBy as ProtoGroupBy,
        ReviewOutcome as ProtoOutcome, ReviewSummary, SessionSummary, ShowReviewRequest,
    },
    commands::resolve_git_dir,
    models::{
        ActorArgs, LocalArgs, ReviewGroupBy, ReviewOutcome, format_comment_json,
        format_diff_hunk_json, format_diff_line_kind, format_identity_display, format_review_json,
        format_review_status, format_session_json, format_session_outcome, format_session_state,
        parse_nonempty, short_sha,
    },
    rpc_client::Client,
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
    New(LocalArgs),
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
    let git_dir = resolve_git_dir()?;
    let client = Client::connect(store).await?;

    match args.command {
        ReviewCommand::List { location, all } => {
            let response = client
                .get_reviews(ListReviewsRequest {
                    git_dir,
                    local: location.local,
                    all,
                    ..Default::default()
                })
                .await?;
            if json {
                let reviews: Vec<Value> = response.reviews.iter().map(format_review_json).collect();
                println!("{}", json!({ "reviews": reviews }));
            } else if response.reviews.is_empty() {
                println!("No reviews found.");
            } else {
                for review in &response.reviews {
                    let scope = if review.local { "local" } else { "remote" };
                    println!(
                        "Review #{}  {}  [{scope}]  {}  ({}..{})",
                        review.number,
                        review.name,
                        format_review_status(review.status.as_known()),
                        short_sha(&review.base_sha),
                        short_sha(&review.head_sha),
                    );
                }
            }
            Ok(())
        }
        ReviewCommand::New(location) => {
            let response = client
                .create_review(CreateReviewRequest {
                    git_dir,
                    local: location.local,
                    ..Default::default()
                })
                .await?;
            let review = response
                .review
                .as_option()
                .context("CreateReview response missing review")?;
            let session = response
                .session
                .as_option()
                .context("CreateReview response missing session")?;
            if json {
                println!(
                    "{}",
                    json!({
                        "review": format_review_json(review),
                        "session": format_session_json(session),
                    })
                );
            } else {
                let scope = if review.local { "local" } else { "remote" };
                println!(
                    "Created review #{}  {}  [{scope}]  (session #{} {})",
                    review.number,
                    review.name,
                    session.number,
                    format_session_state(session.state.as_known()),
                );
            }
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
                .await?;
            let refreshed = response
                .review
                .as_option()
                .context("RefreshReview response missing review")?;
            if json {
                println!(
                    "{}",
                    json!({
                        "review": format_review_json(refreshed),
                        "updated_anchors": response.updated_anchors,
                    })
                );
            } else {
                let scope = if refreshed.local { "local" } else { "remote" };
                println!(
                    "Refreshed review #{}  {}  [{scope}]  Head: {}  ({} anchor(s) updated)",
                    refreshed.number,
                    refreshed.name,
                    short_sha(&refreshed.head_sha),
                    response.updated_anchors,
                );
            }
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
                .get_review_detail(ShowReviewRequest {
                    git_dir,
                    selector: review,
                    local: location.local,
                    group_by: proto_group.into(),
                    unresolved,
                    ..Default::default()
                })
                .await?;
            let summary = response
                .review
                .as_option()
                .context("ShowReview response missing review")?;
            if json {
                let sessions: Vec<Value> =
                    response.sessions.iter().map(format_session_json).collect();
                let comments: Vec<Value> =
                    response.comments.iter().map(format_comment_json).collect();
                let diff_hunks: Vec<Value> = response
                    .diff_hunks
                    .iter()
                    .map(format_diff_hunk_json)
                    .collect();
                println!(
                    "{}",
                    json!({
                        "review": format_review_json(summary),
                        "sessions": sessions,
                        "comments": comments,
                        "diff_hunks": diff_hunks,
                    })
                );
            } else {
                let rendered = render_review_show(
                    summary,
                    &response.sessions,
                    &response.comments,
                    &response.diff_hunks,
                    group_by,
                    unresolved,
                );
                print!("{rendered}");
            }
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
                .await?;
            let closed_review = response
                .review
                .as_option()
                .context("CloseReview response missing review")?;
            let session = response
                .session
                .as_option()
                .context("CloseReview response missing session")?;
            if json {
                println!(
                    "{}",
                    json!({
                        "review": format_review_json(closed_review),
                        "session": format_session_json(session),
                    })
                );
            } else {
                println!(
                    "Closed review #{}  {}  with {} (session #{} {})",
                    closed_review.number,
                    closed_review.name,
                    outcome.as_str(),
                    session.number,
                    format_session_state(session.state.as_known()),
                );
            }
            Ok(())
        }
    }
}

fn render_review_show(
    review: &ReviewSummary,
    sessions: &[SessionSummary],
    comments: &[CommentDetail],
    diff_hunks: &[DiffHunkDetail],
    group_by: ReviewGroupBy,
    unresolved: bool,
) -> String {
    let mut out = String::new();
    let scope = if review.local { "local" } else { "remote" };
    let status = format_review_status(review.status.as_known());
    let unresolved_roots = comments
        .iter()
        .filter(|c| c.reply_to_number.is_none() && !c.resolved)
        .count();
    let unresolved_suffix = if unresolved {
        format!(
            " ({unresolved_roots} unresolved {})",
            if unresolved_roots == 1 {
                "thread"
            } else {
                "threads"
            }
        )
    } else {
        String::new()
    };

    out.push_str(&format!(
        "Review #{}  {}  [{scope}]  {status}{unresolved_suffix}\n",
        review.number, review.name
    ));
    let head_suffix = if review.local { " (snapshot)" } else { "" };
    out.push_str(&format!(
        "Base: {} -> Head: {}{head_suffix}\n",
        short_sha(&review.base_sha),
        short_sha(&review.head_sha),
    ));

    let session_by_id: HashMap<&str, &SessionSummary> =
        sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let comment_by_num: HashMap<u64, &CommentDetail> =
        comments.iter().map(|c| (c.number, c)).collect();
    let hunk_by_root_num: HashMap<u64, &DiffHunkDetail> = comments
        .iter()
        .filter(|c| c.reply_to_number.is_none() && c.anchor.as_option().is_some())
        .zip(diff_hunks.iter())
        .map(|(c, h)| (c.number, h))
        .collect();

    match group_by {
        ReviewGroupBy::File => {
            let roots: Vec<&CommentDetail> = comments
                .iter()
                .filter(|c| c.reply_to_number.is_none())
                .collect();
            let mut replies_by_root: HashMap<u64, Vec<&CommentDetail>> = HashMap::new();
            for comment in comments {
                if let Some(root_num) = comment.reply_to_number {
                    replies_by_root.entry(root_num).or_default().push(comment);
                }
            }

            let mut current_file: Option<&str> = None;
            for (root_idx, root) in roots.iter().enumerate() {
                let file_label = root
                    .anchor
                    .as_option()
                    .map(|a| a.file_path.as_str())
                    .unwrap_or("General");
                if current_file != Some(file_label) {
                    current_file = Some(file_label);
                    out.push('\n');
                    out.push_str(&format_section_banner(file_label));
                    out.push('\n');
                }

                out.push('\n');
                if let Some(anchor) = root.anchor.as_option() {
                    out.push_str(&format_file_anchor_header(anchor, root));
                    out.push('\n');
                    if let Some(hunk) = hunk_by_root_num
                        .get(&root.number)
                        .copied()
                        .or_else(|| find_matching_hunk(diff_hunks, anchor, root_idx))
                    {
                        out.push_str(&format_hunk_lines(hunk));
                        out.push('\n');
                    }
                } else {
                    let res_str = if root.resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    };
                    out.push_str(&format_sub_banner(&format!("general • {res_str}")));
                    out.push('\n');
                }

                out.push_str(&format_comment_in_file_group(root, &session_by_id));
                if let Some(replies) = replies_by_root.get(&root.number) {
                    for reply in replies {
                        out.push('\n');
                        out.push_str(&format_comment_in_file_group(reply, &session_by_id));
                    }
                }
            }
        }
        ReviewGroupBy::Session => {
            for session in sessions {
                let session_comments: Vec<&CommentDetail> = comments
                    .iter()
                    .filter(|c| c.session_id == session.id)
                    .collect();
                if session_comments.is_empty() && session.body.as_deref().is_none_or(str::is_empty)
                {
                    continue;
                }

                let author_label = format_identity_display(session.author.as_option());
                let outcome_label =
                    format_session_outcome(session.outcome.as_known()).unwrap_or("comment");
                let state_label = format_session_state(session.state.as_known());
                let header_title = format!(
                    "Session #{} • {author_label} • {outcome_label} • {state_label}",
                    session.number
                );
                out.push('\n');
                out.push_str(&format_section_banner(&header_title));
                out.push('\n');

                if let Some(body) = session.body.as_deref().filter(|b| !b.trim().is_empty()) {
                    out.push_str(&format!("  {}\n", body.trim()));
                }

                for comment in session_comments {
                    out.push('\n');
                    let effective_anchor = comment.anchor.as_option().or_else(|| {
                        comment
                            .reply_to_number
                            .and_then(|num| comment_by_num.get(&num))
                            .and_then(|root| root.anchor.as_option())
                    });
                    let effective_root = comment
                        .reply_to_number
                        .and_then(|num| comment_by_num.get(&num).copied())
                        .unwrap_or(comment);

                    if let Some(anchor) = effective_anchor {
                        out.push_str(&format_session_anchor_header(anchor, effective_root));
                        out.push('\n');
                        if comment.reply_to_number.is_none()
                            && let Some(hunk) = hunk_by_root_num
                                .get(&comment.number)
                                .copied()
                                .or_else(|| find_matching_hunk(diff_hunks, anchor, usize::MAX))
                        {
                            out.push_str(&format_hunk_lines(hunk));
                            out.push('\n');
                        }
                    }

                    let author_str = format_identity_display(comment.author.as_option());
                    if let Some(reply_num) = comment.reply_to_number {
                        out.push_str(&format!(
                            "  [{}] {author_str} (reply to #{reply_num})\n",
                            comment.number
                        ));
                    } else {
                        out.push_str(&format!("  [{}] {author_str}\n", comment.number));
                    }
                    for line in comment.body.lines() {
                        out.push_str(&format!("    {line}\n"));
                    }
                }
            }
        }
    }

    out
}

fn format_section_banner(title: &str) -> String {
    let prefix = format!("══ {title} ");
    let pad = 72usize.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", "═".repeat(pad.max(3)))
}

fn format_sub_banner(title: &str) -> String {
    let prefix = format!("── {title} ");
    let pad = 72usize.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", "─".repeat(pad.max(3)))
}

fn format_line_range(anchor: &CommentAnchorDetail) -> String {
    match anchor.start_line {
        Some(start) if start < anchor.line => format!("lines {start}-{}", anchor.line),
        _ => format!("line {}", anchor.line),
    }
}

fn format_file_anchor_header(anchor: &CommentAnchorDetail, root: &CommentDetail) -> String {
    let range = format_line_range(anchor);
    let status = if root.resolved {
        "resolved"
    } else {
        "unresolved"
    };
    let outdated = if root.outdated { " • outdated" } else { "" };
    format_sub_banner(&format!("{range} • {status}{outdated}"))
}

fn format_session_anchor_header(anchor: &CommentAnchorDetail, root: &CommentDetail) -> String {
    let line_part = match anchor.start_line {
        Some(start) if start < anchor.line => format!("{start}-{}", anchor.line),
        _ => anchor.line.to_string(),
    };
    let status = if root.resolved {
        "resolved"
    } else {
        "unresolved"
    };
    let outdated = if root.outdated { " • outdated" } else { "" };
    format_sub_banner(&format!(
        "{}:{line_part} • {status}{outdated}",
        anchor.file_path
    ))
}

fn find_matching_hunk<'a>(
    diff_hunks: &'a [DiffHunkDetail],
    anchor: &CommentAnchorDetail,
    fallback_idx: usize,
) -> Option<&'a DiffHunkDetail> {
    if let Some(hunk) = diff_hunks
        .get(fallback_idx)
        .filter(|h| h.file_path == anchor.file_path)
    {
        return Some(hunk);
    }
    diff_hunks.iter().find(|h| {
        h.file_path == anchor.file_path
            && h.lines
                .iter()
                .any(|l| l.line_number == anchor.line || Some(l.line_number) == anchor.start_line)
    })
}

fn format_hunk_lines(hunk: &DiffHunkDetail) -> String {
    let mut lines = Vec::with_capacity(hunk.lines.len());
    for line in &hunk.lines {
        let (_, marker) = format_diff_line_kind(line.kind.as_known());
        lines.push(format!(
            " {marker} {:>2} │ {}",
            line.line_number, line.content
        ));
    }
    lines.join("\n")
}

fn format_comment_in_file_group(
    comment: &CommentDetail,
    session_by_id: &HashMap<&str, &SessionSummary>,
) -> String {
    let author_label = format_identity_display(comment.author.as_option());
    let session_info = session_by_id
        .get(comment.session_id.as_str())
        .map(|s| {
            format!(
                " • session #{} ({})",
                s.number,
                format_session_state(s.state.as_known())
            )
        })
        .unwrap_or_default();
    let mut out = format!("  [{}] {author_label}{session_info}\n", comment.number);
    for line in comment.body.lines() {
        out.push_str(&format!("    {line}\n"));
    }
    out
}
