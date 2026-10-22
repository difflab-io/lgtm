//! Stateless GitHub SDK and gh CLI helpers. Domain mapping stays in forge::github.
use crate::error::{Result, invalid};
use octocrab::{Octocrab, Page};
use serde_json::{Value, json};
use std::time::Duration;

/// Captures stdout privately; gh diagnostics may contain account details and are not forwarded.
pub async fn get_token(account: Option<&str>) -> Result<String> {
    if account.is_some_and(|v| v.is_empty() || v.starts_with('-') || v.contains(['\n', '\r'])) {
        return Err(invalid("Invalid GitHub account"));
    }
    let mut command = tokio::process::Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    if let Some(account) = account {
        // Explicit account selection must not silently use an environment token.
        command.args(["--user", account]);
        command.env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
    }
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| invalid("GitHub credential lookup timed out"))?
        .map_err(|_| invalid("GitHub requires gh installed and authenticated"))?;
    if !output.status.success() {
        return Err(invalid(
            "GitHub credentials unavailable; run gh auth login for the selected account",
        ));
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|_| invalid("Invalid GitHub credential encoding"))?;
    let token = token.trim();
    if token.is_empty() || token.contains(['\n', '\r']) {
        return Err(invalid("GitHub credential lookup returned no usable token"));
    }
    Ok(token.to_owned())
}

/// Follow GitHub Link headers through Octocrab, with a bounded complete observation.
pub async fn get_all(client: &Octocrab, route: &str) -> Result<Vec<Value>> {
    let mut page: Page<Value> = request(client.get(route, Some(&json!({"per_page":100})))).await?;
    let mut entries = page.take_items();
    for _ in 1..1000 {
        let Some(mut next) = request(client.get_page::<Value>(&page.next)).await? else {
            return Ok(entries);
        };
        entries.append(&mut next.take_items());
        page = next;
    }
    if page.next.is_none() {
        Ok(entries)
    } else {
        Err(invalid(
            "GitHub pagination exceeded safety limit; snapshot not applied",
        ))
    }
}

/// Read all files marked Viewed by the authenticated GitHub viewer.
pub async fn get_viewed_files(
    client: &Octocrab,
    owner: &str,
    name: &str,
    number: i64,
) -> Result<Vec<String>> {
    let mut cursor = Value::Null;
    let mut marks = Vec::new();
    for _ in 0..1000 {
        let response: Value = request(client.graphql(&json!({"query": "query($owner:String!,$name:String!,$number:Int!,$cursor:String){repository(owner:$owner,name:$name){pullRequest(number:$number){files(first:100,after:$cursor){nodes{path viewerViewedState}pageInfo{hasNextPage endCursor}}}}}", "variables":{"owner":owner,"name":name,"number":number,"cursor":cursor}}))).await?;
        let files = &response["repository"]["pullRequest"]["files"];
        for file in files["nodes"]
            .as_array()
            .ok_or_else(|| invalid("Incomplete GitHub viewed-file response"))?
        {
            if file["viewerViewedState"] == "VIEWED" {
                marks.push(text(file, "path")?);
            }
        }
        if !files["pageInfo"]["hasNextPage"]
            .as_bool()
            .ok_or_else(|| invalid("Incomplete GitHub viewed-file page"))?
        {
            return Ok(marks);
        }
        cursor = Value::String(text(&files["pageInfo"], "endCursor")?);
    }
    Err(invalid(
        "GitHub viewed-file pagination exceeded its safety bound",
    ))
}

/// Set GitHub's per-viewer file progress. The provider enforces permissions.
pub async fn set_file_viewed(
    client: &Octocrab,
    owner: &str,
    name: &str,
    number: i64,
    path: &str,
    viewed: bool,
) -> Result<()> {
    let response: Value = request(client.graphql(&json!({"query":"query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id}}}","variables":{"owner":owner,"name":name,"number":number}}))).await?;
    let pr = text(&response["repository"]["pullRequest"], "id")?;
    let (mutation, input) = if viewed {
        ("markFileAsViewed", "MarkFileAsViewedInput")
    } else {
        ("unmarkFileAsViewed", "UnmarkFileAsViewedInput")
    };
    let _: Value = request(client.graphql(&json!({"query":format!("mutation($input:{input}!){{{mutation}(input:$input){{clientMutationId}}}}"),"variables":{"input":{"pullRequestId":pr,"path":path}}}))).await?;
    Ok(())
}

/// Read one comment within its previously verified thread, including private drafts.
/// A missing node is not an authorization receipt; callers verify scope before deletion recovery.
pub async fn get_review_comment(
    client: &Octocrab,
    thread_id: &str,
    comment_id: &str,
) -> Result<Option<Value>> {
    let mut cursor = Value::Null;
    loop {
        let response: Value = request(client.graphql(
            &json!({"query":COMMENTS_QUERY,"variables":{"id":thread_id,"cursor":cursor}}),
        ))
        .await?;
        if response.get("node") == Some(&Value::Null) {
            return Ok(None);
        }
        let connection = &response["node"]["comments"];
        let nodes = connection["nodes"]
            .as_array()
            .ok_or_else(|| invalid("Review thread comments unavailable"))?;
        for comment in nodes {
            if opaque(comment, "databaseId")? == comment_id {
                return Ok(Some(comment.clone()));
            }
        }
        if !connection["pageInfo"]["hasNextPage"]
            .as_bool()
            .ok_or_else(|| invalid("Review comment pagination is incomplete"))?
        {
            return Ok(None);
        }
        cursor = Value::String(text(&connection["pageInfo"], "endCursor")?);
    }
}

/// Observe only the selected thread when recovering an uncertain resolution.
pub async fn get_thread_resolution(client: &Octocrab, thread_id: &str) -> Result<Option<bool>> {
    let response: Value = request(client.graphql(&json!({"query":"query($id:ID!){node(id:$id){... on PullRequestReviewThread{id isResolved}}}","variables":{"id":thread_id}}))).await?;
    if response.get("node") == Some(&Value::Null) {
        return Ok(None);
    }
    if response["node"]["id"] != thread_id {
        return Err(invalid("Thread observation returned another object"));
    }
    response["node"]["isResolved"]
        .as_bool()
        .map(Some)
        .ok_or_else(|| invalid("Thread resolution unavailable"))
}

pub(crate) async fn request<T>(
    future: impl std::future::Future<Output = octocrab::Result<T>>,
) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .map_err(|_| invalid("GitHub request timed out; reconcile before retrying writes"))?
        .map_err(|e| {
            // Do not forward HTTP request bodies, tokens, or provider diagnostics.
            if let octocrab::Error::GitHub { source, .. } = &e {
                let reason = if source.status_code.as_u16() == 422 {
                    "review data was rejected; check the review outcome and comment locations"
                } else {
                    "request rejected; check account permissions/rate limits"
                };
                invalid(format!("GitHub HTTP {}: {reason}", source.status_code))
            } else {
                invalid("GitHub transport failed; reconcile before retrying writes")
            }
        })
}

pub(crate) fn text(v: &Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("GitHub response missing {key}")))
}
pub(crate) fn number(v: &Value, key: &str) -> Result<i64> {
    v[key]
        .as_i64()
        .ok_or_else(|| invalid(format!("GitHub response missing {key}")))
}
pub(crate) fn opaque(v: &Value, key: &str) -> Result<String> {
    if let Some(v) = v[key].as_str()
        && !v.is_empty()
    {
        return Ok(v.into());
    }
    v[key]
        .as_u64()
        .map(|n| n.to_string())
        .ok_or_else(|| invalid(format!("GitHub response missing {key}")))
}

pub(crate) const COMMENTS_QUERY: &str = r#"query($id:ID!,$cursor:String){node(id:$id){... on PullRequestReviewThread{comments(first:100,after:$cursor){nodes{id databaseId viewerDidAuthor path body updatedAt author{login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{databaseId state}} pageInfo{hasNextPage endCursor}}}}}"#;
