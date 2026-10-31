//! Stateless `octocrab` adapter for GitHub REST and GraphQL pull request, review, comment, and viewed-file APIs.

use std::{collections::BTreeMap, sync::OnceLock};

use chrono::{DateTime, TimeZone, Utc};
use octocrab::Octocrab;
use serde_json::{Value, json};

use crate::{
    error::{Error, Result},
    extensions::gitoxide::HeadCommitPayload,
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
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
        })
        .or_else(|| {
            if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() == Ok("1") {
                return None;
            }
            cached_gh_cli_token()
        });
    if let Some(tok) = resolved_token {
        builder = builder.personal_token(tok);
    }
    let resolved_base = api_base_url
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("LGTM_GITHUB_TEST_API_URL")
                .or_else(|_| std::env::var("GITHUB_API_URL"))
                .ok()
                .map(|u| u.trim().to_owned())
                .filter(|u| !u.is_empty())
        });
    if let Some(base) = resolved_base {
        builder = builder
            .base_uri(&base)
            .map_err(|err| Error::Forge(format!("invalid GitHub API base URI '{base}': {err}")))?;
    }
    builder
        .build()
        .map_err(|err| Error::Forge(format!("failed to initialize octocrab client: {err}")))
}

/// Push a committed head tree and commit to `refs/heads/<branch>` on GitHub via the Git Data API.
pub async fn push_commit_to_branch(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    branch: &str,
    payload: &HeadCommitPayload,
) -> Result<String> {
    let parent_route = format!("/repos/{owner}/{repo}/git/commits/{}", payload.parent_sha);
    let parent_json: Value = client.get(parent_route, None::<&()>).await.map_err(|err| {
        Error::Forge(format!(
            "failed to read parent commit '{}': {err}",
            payload.parent_sha
        ))
    })?;
    let base_tree_sha = parent_json
        .pointer("/tree/sha")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Forge("parent commit response missing tree.sha".to_owned()))?;

    let mut tree_entries = Vec::new();
    for (path, maybe_content) in &payload.changed_files {
        match maybe_content {
            Some(content) => {
                tree_entries.push(json!({
                    "path": path,
                    "mode": "100644",
                    "type": "blob",
                    "content": content,
                }));
            }
            None => {
                tree_entries.push(json!({
                    "path": path,
                    "mode": "100644",
                    "type": "blob",
                    "sha": Value::Null,
                }));
            }
        }
    }

    let new_tree_sha = if tree_entries.is_empty() {
        base_tree_sha.to_owned()
    } else {
        let tree_route = format!("/repos/{owner}/{repo}/git/trees");
        let tree_json: Value = client
            .post(
                tree_route,
                Some(&json!({
                    "base_tree": base_tree_sha,
                    "tree": tree_entries,
                })),
            )
            .await
            .map_err(|err| Error::Forge(format!("failed to create remote Git tree: {err}")))?;
        tree_json
            .get("sha")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Forge("remote tree response missing sha".to_owned()))?
            .to_owned()
    };

    let date_str = Utc
        .timestamp_opt(payload.timestamp, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let commit_route = format!("/repos/{owner}/{repo}/git/commits");
    let commit_json: Value = client
        .post(
            commit_route,
            Some(&json!({
                "message": payload.message.trim_end_matches('\n'),
                "tree": new_tree_sha,
                "parents": [payload.parent_sha],
                "author": {
                    "name": payload.author_name,
                    "email": payload.author_email,
                    "date": date_str,
                },
                "committer": {
                    "name": payload.author_name,
                    "email": payload.author_email,
                    "date": date_str,
                },
            })),
        )
        .await
        .map_err(|err| Error::Forge(format!("failed to create remote Git commit: {err}")))?;
    let commit_sha = commit_json
        .get("sha")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Forge("remote commit response missing sha".to_owned()))?
        .to_owned();

    let ref_route = format!("/repos/{owner}/{repo}/git/refs/heads/{branch}");
    let _: Value = client
        .patch(
            ref_route,
            Some(&json!({
                "sha": commit_sha,
                "force": true,
            })),
        )
        .await
        .map_err(|err| Error::Forge(format!("failed to update remote ref '{branch}': {err}")))?;

    let pulls_route = format!("/repos/{owner}/{repo}/pulls?head={owner}:{branch}&state=all");
    for _ in 0..20 {
        if let Ok(prs) = client
            .get::<Vec<Value>, _, _>(&pulls_route, None::<&()>)
            .await
            && (prs.is_empty()
                || prs
                    .first()
                    .and_then(|pr| pr.pointer("/head/sha"))
                    .and_then(Value::as_str)
                    == Some(commit_sha.as_str()))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    Ok(commit_sha)
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

/// Create a remote draft comment on a pull request.
pub async fn create_draft_comment(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    pending_review_id: Option<&str>,
    input: &ForgeDraftCommentInput,
) -> Result<ForgeDraftCommentReceipt> {
    let response: Value = if let Some(reply_to) = input
        .reply_to_forge_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/comments/{reply_to}/replies");
        client
            .post(route, Some(&json!({ "body": input.body })))
            .await
            .map_err(|err| Error::Forge(format!("failed to create remote reply: {err}")))?
    } else if let Some(path) = &input.file_path {
        let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/comments");
        let mut payload = json!({
            "body": input.body,
            "commit_id": input.commit_sha,
            "path": path,
        });
        if let Some(line) = input.line {
            payload["line"] = Value::Number(line.into());
            payload["side"] = Value::String("RIGHT".to_owned());
        }
        if let Some(start_line) = input.start_line
            && input.line.is_none_or(|end| start_line < end)
        {
            payload["start_line"] = Value::Number(start_line.into());
            payload["start_side"] = Value::String("RIGHT".to_owned());
        }
        client
            .post(route, Some(&payload))
            .await
            .map_err(|err| Error::Forge(format!("failed to create remote comment: {err}")))?
    } else {
        let route = format!("/repos/{owner}/{repo}/issues/{pr_number}/comments");
        client
            .post(route, Some(&json!({ "body": input.body })))
            .await
            .map_err(|err| Error::Forge(format!("failed to create remote comment: {err}")))?
    };

    let comment_forge_id = response
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_owned())
        .ok_or_else(|| Error::Forge("GitHub comment response did not include 'id'".to_owned()))?;
    let review_id = pending_review_id
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            response
                .get("pull_request_review_id")
                .map(|v| v.to_string().trim_matches('"').to_owned())
                .filter(|v| v != "null" && !v.is_empty())
        })
        .unwrap_or_else(|| format!("pending-{pr_number}"));

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
    _pending_review_id: Option<&str>,
    outcome: ReviewOutcome,
    body: Option<&str>,
) -> Result<ForgeReviewReceipt> {
    let event = match outcome {
        ReviewOutcome::Approve => "APPROVE",
        ReviewOutcome::Request | ReviewOutcome::Reject => "REQUEST_CHANGES",
        ReviewOutcome::Comment => "COMMENT",
    };

    let route = format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews");
    let review_body = body
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Reviewed via LGTM");
    let payload = json!({
        "event": event,
        "body": review_body,
    });
    let response: Value = client
        .post(route, Some(&payload))
        .await
        .map_err(|err| Error::Forge(format!("failed to submit review: {err}")))?;

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

fn cached_gh_cli_token() -> Option<String> {
    static GH_CLI_TOKEN: OnceLock<Option<String>> = OnceLock::new();
    GH_CLI_TOKEN
        .get_or_init(|| {
            std::process::Command::new("gh")
                .args(["auth", "token"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        })
        .clone()
}
