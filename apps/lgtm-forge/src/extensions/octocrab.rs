//! Stateless `octocrab` adapter for GitHub REST and GraphQL pull request, review, comment, and viewed-file APIs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use octocrab::Octocrab;
use serde_json::{Value, json};

use crate::{
    error::{Error, Result},
    forge::models::{
        ForgeCommentUpdate, ForgeDraftCommentInput, ForgeDraftCommentReceipt, ForgePullRequest,
        ForgePullRequestBundle, ForgeReviewComment, ForgeReviewReceipt, ForgeReviewSession,
    },
    models::{ReviewOutcome, ReviewStatus, SessionState},
};

/// Build an `Octocrab` client configured with optional authentication token and API base URL.
pub fn build_client(token: Option<&str>, api_base_url: Option<&str>) -> Result<Octocrab> {
    let mut builder = Octocrab::builder();
    let resolved_token = token
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("GITHUB_TOKEN")
                .or_else(|_| std::env::var("GH_TOKEN"))
                .ok()
                .filter(|t| !t.trim().is_empty())
        });
    if let Some(tok) = resolved_token {
        builder = builder.personal_token(tok);
    }
    if let Some(base) = api_base_url.filter(|u| !u.trim().is_empty()) {
        builder = builder
            .base_uri(base)
            .map_err(|err| Error::Forge(format!("invalid GitHub API base URI '{base}': {err}")))?;
    }
    builder
        .build()
        .map_err(|err| Error::Forge(format!("failed to initialize octocrab client: {err}")))
}

/// Fetch a pull request bundle (metadata, review sessions, comments, and viewed-file states) from GitHub.
pub async fn fetch_pull_request_bundle(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    branch_or_pr: &str,
) -> Result<ForgePullRequestBundle> {
    let pr_number = resolve_pr_number(client, owner, repo, branch_or_pr).await?;
    let pr_route = format!("/repos/{owner}/{repo}/pulls/{pr_number}");
    let pr_json: Value = client
        .get(pr_route, None::<&()>)
        .await
        .map_err(|err| Error::Forge(format!("failed to fetch PR #{pr_number}: {err}")))?;

    let forge_id = pr_json
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let branch = pr_json
        .pointer("/head/ref")
        .and_then(Value::as_str)
        .unwrap_or("HEAD")
        .to_owned();
    let base_sha = pr_json
        .pointer("/base/sha")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let head_sha = pr_json
        .pointer("/head/sha")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let merged = pr_json
        .get("merged")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state_str = pr_json
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("open");
    let status = if merged {
        ReviewStatus::Merged
    } else if state_str.eq_ignore_ascii_case("closed") {
        ReviewStatus::Closed
    } else {
        ReviewStatus::Open
    };

    let reviews_route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews");
    let reviews_json: Vec<Value> = client
        .get(reviews_route, None::<&()>)
        .await
        .map_err(|err| Error::Forge(format!("failed to fetch PR #{pr_number} reviews: {err}")))?;

    let mut sessions = Vec::new();
    for item in reviews_json {
        let Some(id_val) = item.get("id") else {
            continue;
        };
        let session_forge_id = id_val.to_string().trim_matches('"').to_owned();
        let login = item
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("github-user")
            .to_owned();
        let raw_state = item
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("COMMENTED");
        let (state, outcome) = match raw_state.to_ascii_uppercase().as_str() {
            "PENDING" => (SessionState::Draft, None),
            "APPROVED" => (SessionState::Published, Some(ReviewOutcome::Approve)),
            "CHANGES_REQUESTED" => (SessionState::Published, Some(ReviewOutcome::Request)),
            _ => (SessionState::Published, Some(ReviewOutcome::Comment)),
        };
        let body = item
            .get("body")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let published_at = item
            .get("submitted_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        sessions.push(ForgeReviewSession {
            forge_id: session_forge_id,
            author_name: login.clone(),
            author_email: format!("{login}@users.noreply.github.com"),
            author_login: login,
            state,
            outcome,
            body,
            published_at,
        });
    }

    let comments_route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/comments");
    let comments_json: Vec<Value> = client
        .get(comments_route, None::<&()>)
        .await
        .map_err(|err| Error::Forge(format!("failed to fetch PR #{pr_number} comments: {err}")))?;

    let mut comments = Vec::new();
    for item in comments_json {
        let Some(id_val) = item.get("id") else {
            continue;
        };
        let comment_forge_id = id_val.to_string().trim_matches('"').to_owned();
        let session_forge_id = item
            .get("pull_request_review_id")
            .map(|v| v.to_string().trim_matches('"').to_owned())
            .filter(|v| v != "null" && !v.is_empty())
            .unwrap_or_else(|| format!("default-session-{pr_number}"));
        let reply_to_forge_id = item
            .get("in_reply_to_id")
            .map(|v| v.to_string().trim_matches('"').to_owned())
            .filter(|v| v != "null" && !v.is_empty());
        let login = item
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("github-user")
            .to_owned();
        let file_path = item.get("path").and_then(Value::as_str).map(str::to_owned);
        let start_line = item
            .get("start_line")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let line = item
            .get("line")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let commit_sha = item
            .get("commit_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let original_start_line = item
            .get("original_start_line")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let original_line = item
            .get("original_line")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .or(line);
        let original_commit_sha = item
            .get("original_commit_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| commit_sha.clone());
        let body = item
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let outdated = line.is_none() && original_line.is_some();

        comments.push(ForgeReviewComment {
            forge_id: comment_forge_id,
            session_forge_id,
            reply_to_forge_id,
            author_name: login.clone(),
            author_email: format!("{login}@users.noreply.github.com"),
            author_login: login,
            file_path,
            start_line,
            line: line.or(original_line),
            commit_sha,
            original_start_line,
            original_line,
            original_commit_sha,
            body,
            resolved: false,
            outdated,
        });
    }

    Ok(ForgePullRequestBundle {
        pull_request: ForgePullRequest {
            pr_number,
            forge_id,
            branch,
            base_sha,
            head_sha,
            status,
            reviewed_files: BTreeMap::new(),
        },
        sessions,
        comments,
    })
}

/// Create a pending review on GitHub and return its `forge_id`.
pub async fn create_pending_review(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    head_sha: &str,
) -> Result<String> {
    let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews");
    let response: Value = client
        .post(route, Some(&json!({ "commit_id": head_sha })))
        .await
        .map_err(|err| Error::Forge(format!("failed to create pending review: {err}")))?;
    let review_id = response
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_owned())
        .ok_or_else(|| Error::Forge("GitHub review response did not include 'id'".to_owned()))?;
    Ok(review_id)
}

/// Create a remote draft comment on a pull request (ensuring a pending review exists first).
pub async fn create_draft_comment(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    pending_review_id: Option<&str>,
    input: &ForgeDraftCommentInput,
) -> Result<ForgeDraftCommentReceipt> {
    let review_id = match pending_review_id.filter(|s| !s.trim().is_empty()) {
        Some(existing) => existing.to_owned(),
        None => create_pending_review(client, owner, repo, pr_number, &input.commit_sha).await?,
    };
    let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/comments");
    let mut payload = json!({
        "body": input.body,
        "commit_id": input.commit_sha,
    });
    if let Ok(review_id_num) = review_id.parse::<u64>() {
        payload["pull_request_review_id"] = Value::Number(review_id_num.into());
    } else {
        payload["pull_request_review_id"] = Value::String(review_id.clone());
    }
    if let Some(path) = &input.file_path {
        payload["path"] = Value::String(path.clone());
    }
    if let Some(line) = input.line {
        payload["line"] = Value::Number(line.into());
        payload["side"] = Value::String("RIGHT".to_owned());
    }
    if let Some(start_line) = input.start_line {
        payload["start_line"] = Value::Number(start_line.into());
        payload["start_side"] = Value::String("RIGHT".to_owned());
    }
    if let Some(reply_to) = &input.reply_to_forge_id
        && let Ok(reply_id) = reply_to.parse::<u64>()
    {
        payload["in_reply_to"] = Value::Number(reply_id.into());
    }

    let response: Value = client
        .post(route, Some(&payload))
        .await
        .map_err(|err| Error::Forge(format!("failed to create remote comment: {err}")))?;
    let comment_forge_id = response
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_owned())
        .ok_or_else(|| Error::Forge("GitHub comment response did not include 'id'".to_owned()))?;

    Ok(ForgeDraftCommentReceipt {
        pending_review_forge_id: review_id,
        comment_forge_id,
    })
}

/// Update an existing pull request comment on GitHub.
pub async fn update_comment(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    forge_id: &str,
    update: &ForgeCommentUpdate,
) -> Result<()> {
    if let Some(body) = &update.body {
        let route = format!("/repos/{owner}/{repo}/pulls/comments/{forge_id}");
        let _: Value = client
            .patch(route, Some(&json!({ "body": body })))
            .await
            .map_err(|err| Error::Forge(format!("failed to update comment '{forge_id}': {err}")))?;
    }
    Ok(())
}

/// Delete a pull request comment on GitHub.
pub async fn delete_comment(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    forge_id: &str,
) -> Result<()> {
    let route = format!("/repos/{owner}/{repo}/pulls/comments/{forge_id}");
    let _: Value = client
        ._delete(route, None::<&()>)
        .await
        .map(|resp| json!({ "status": resp.status().as_u16() }))
        .map_err(|err| Error::Forge(format!("failed to delete comment '{forge_id}': {err}")))?;
    Ok(())
}

/// Mark or unmark a pull request file as viewed on GitHub via GraphQL.
pub async fn set_file_viewed(
    client: &Octocrab,
    pr_node_id: &str,
    file_path: &str,
    viewed: bool,
) -> Result<()> {
    let mutation = if viewed {
        "mutation($prId: ID!, $path: String!) { markFileAsViewed(input: { pullRequestId: $prId, path: $path }) { clientMutationId } }"
    } else {
        "mutation($prId: ID!, $path: String!) { unmarkFileAsViewed(input: { pullRequestId: $prId, path: $path }) { clientMutationId } }"
    };
    let _: Value = client
        .graphql(&json!({
            "query": mutation,
            "variables": {
                "prId": pr_node_id,
                "path": file_path,
            }
        }))
        .await
        .map_err(|err| Error::Forge(format!("failed to sync viewed file state: {err}")))?;
    Ok(())
}

/// Submit a pending review (or create and immediately submit a review) on GitHub.
pub async fn submit_review(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    pending_review_id: Option<&str>,
    outcome: ReviewOutcome,
    body: Option<&str>,
) -> Result<ForgeReviewReceipt> {
    let event = match outcome {
        ReviewOutcome::Approve => "APPROVE",
        ReviewOutcome::Request | ReviewOutcome::Reject => "REQUEST_CHANGES",
        ReviewOutcome::Comment => "COMMENT",
    };

    let response: Value =
        if let Some(review_id) = pending_review_id.filter(|s| !s.trim().is_empty()) {
            let route =
                format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews/{review_id}/events");
            let mut payload = json!({ "event": event });
            if let Some(text) = body {
                payload["body"] = Value::String(text.to_owned());
            }
            client.post(route, Some(&payload)).await.map_err(|err| {
                Error::Forge(format!("failed to submit review '{review_id}': {err}"))
            })?
        } else {
            let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews");
            let mut payload = json!({ "event": event });
            if let Some(text) = body {
                payload["body"] = Value::String(text.to_owned());
            }
            client
                .post(route, Some(&payload))
                .await
                .map_err(|err| Error::Forge(format!("failed to submit review: {err}")))?
        };

    let review_forge_id = response
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_owned())
        .unwrap_or_else(|| format!("review-{pr_number}"));

    Ok(ForgeReviewReceipt {
        review_forge_id,
        published_at: Utc::now(),
    })
}

async fn resolve_pr_number(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    branch_or_pr: &str,
) -> Result<u64> {
    let trimmed = branch_or_pr.trim();
    if let Some(num) = trimmed
        .strip_prefix("PR")
        .or_else(|| trimmed.strip_prefix("pr"))
        .and_then(|rest| rest.split('-').next())
        .and_then(|digits| digits.parse::<u64>().ok())
    {
        return Ok(num);
    }
    if let Some((_, num_part)) = trimmed.rsplit_once('#')
        && let Ok(num) = num_part.parse::<u64>()
    {
        return Ok(num);
    }
    if let Ok(num) = trimmed.parse::<u64>() {
        return Ok(num);
    }

    let route = format!("/repos/{owner}/{repo}/pulls?head={owner}:{trimmed}&state=all");
    let prs: Vec<Value> = client.get(route, None::<&()>).await.map_err(|err| {
        Error::Forge(format!("failed to query PRs for branch '{trimmed}': {err}"))
    })?;
    prs.first()
        .and_then(|pr| pr.get("number"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "no pull request found on '{owner}/{repo}' for '{trimmed}'"
            ))
        })
}
