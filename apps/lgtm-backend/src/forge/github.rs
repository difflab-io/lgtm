//! Octocrab-backed GitHub REST/GraphQL API with complete paginated snapshots.
use crate::{
    error::{Error, Result, invalid},
    models::{Anchor, ContextKey, ForgeRepositoryUrl},
    utils::{ForgeSnapshot, PullRequest, RemoteAuthor, RemoteComment, RemoteThread},
};
use octocrab::Octocrab;
use serde_json::{Value, json};
use std::time::Duration;

/// Validated github.com repository scope. IDs remain scoped opaque strings.
#[derive(Clone, Debug)]
pub struct Repository {
    pub owner: String,
    pub name: String,
    pub repository_url: String,
}

impl Repository {
    pub fn route(&self) -> String {
        format!("/repos/{}/{}", self.owner, self.name)
    }

    pub fn parse(repository_url: &str) -> Result<Self> {
        let repository_url = ForgeRepositoryUrl::parse(repository_url)?;
        let url = url::Url::parse(repository_url.as_str())
            .map_err(|_| invalid("Invalid repository URL"))?;
        if url.scheme() != "https" || url.host_str() != Some("github.com") || url.port().is_some() {
            return Err(invalid(
                "Only https://github.com repositories are supported",
            ));
        }
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|v| {
                v.is_empty()
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(invalid("GitHub repository URL requires owner/repository"));
        }
        let name = parts[1].trim_end_matches(".git");
        if name.is_empty() {
            return Err(invalid("GitHub repository name is empty"));
        }
        Ok(Self {
            owner: parts[0].into(),
            name: name.into(),
            repository_url: format!("https://github.com/{}/{}", parts[0], name),
        })
    }

    pub fn from_context(key: &ContextKey) -> Result<(Self, i64)> {
        match key {
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => Ok((Self::parse(repository_url.as_str())?, *number)),
            _ => Err(invalid("Remote operations require a forge-backed context")),
        }
    }
}

/// Cloneable provider. Test construction always uses an inert fixture token.
#[derive(Clone)]
pub struct Github {
    client: Octocrab,
    api_url: String,
}

impl Github {
    pub async fn get_pull_requests(
        &self,
        repo: &Repository,
        query: &str,
        page: u32,
    ) -> Result<(Vec<PullRequest>, u32)> {
        let page = page.max(1);
        // Search is server-side, not a misleading filter of one REST page.
        if !query.trim().is_empty() {
            let result = self.get("/search/issues", Some(&json!({"q":format!("repo:{}/{} is:pr is:open {}", repo.owner, repo.name, query),"per_page":30,"page":page}))).await?;
            let mut entries = vec![];
            let items = result["items"]
                .as_array()
                .ok_or_else(|| invalid("Malformed GitHub search response"))?;
            for item in items {
                entries.push(self.get_pull_request(repo, number(item, "number")?).await?);
            }
            let next = if u64::from(page) * 30 < result["total_count"].as_u64().unwrap_or(0) {
                page + 1
            } else {
                0
            };
            return Ok((entries, next));
        }
        let response = self
            .get(
                &format!("{}/pulls", repo.route()),
                Some(&json!({"state":"open","per_page":30,"page":page})),
            )
            .await?;
        let items = response
            .as_array()
            .ok_or_else(|| invalid("Malformed GitHub PR list"))?;
        let entries = items
            .iter()
            .map(|v| pull_request(repo, v))
            .collect::<Result<Vec<_>>>()?;
        Ok((entries, if items.len() == 30 { page + 1 } else { 0 }))
    }

    pub async fn get_pull_request(&self, repo: &Repository, pr: i64) -> Result<PullRequest> {
        pull_request(
            repo,
            &self
                .get(&format!("{}/pulls/{pr}", repo.route()), None)
                .await?,
        )
    }

    pub async fn get_account_id(&self) -> Result<String> {
        opaque(&self.get("/user", None).await?, "id")
    }

    pub async fn get_snapshot(&self, repo: &Repository, pr: i64) -> Result<ForgeSnapshot> {
        let initial = self.get_pull_request(repo, pr).await?;
        let mut threads = vec![];
        let mut cursor = Value::Null;
        loop {
            let response = self.graphql(json!({"query":THREADS_QUERY,"variables":{"owner":repo.owner,"name":repo.name,"number":pr,"cursor":cursor}})).await?;
            let connection = &response["repository"]["pullRequest"]["reviewThreads"];
            let nodes = connection["nodes"]
                .as_array()
                .ok_or_else(|| invalid("Incomplete GitHub thread response"))?;
            for node in nodes {
                let id = text(node, "id")?;
                let mut comments = node["comments"]["nodes"]
                    .as_array()
                    .ok_or_else(|| invalid("Missing GitHub thread comments"))?
                    .clone();
                let mut comments_info = node["comments"]["pageInfo"].clone();
                while comments_info["hasNextPage"] == true {
                    let after = text(&comments_info, "endCursor")?;
                    let result = self
                        .graphql(
                            json!({"query":COMMENTS_QUERY,"variables":{"id":id,"cursor":after}}),
                        )
                        .await?;
                    let part = &result["node"]["comments"];
                    comments.extend(
                        part["nodes"]
                            .as_array()
                            .ok_or_else(|| invalid("Incomplete GitHub comment page"))?
                            .clone(),
                    );
                    comments_info = part["pageInfo"].clone();
                }
                if comments.is_empty() {
                    return Err(invalid("GitHub returned empty review thread"));
                }
                let root_id = opaque(&comments[0], "databaseId")?;
                let mapped = comments
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        remote_comment(
                            c,
                            "review_comment",
                            if i > 0 { Some(root_id.clone()) } else { None },
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                let path = text(node, "path")?;
                let anchor = if node["subjectType"] == "FILE" {
                    Anchor::File { path }
                } else {
                    let end = node["line"]
                        .as_i64()
                        .or_else(|| node["originalLine"].as_i64())
                        .ok_or_else(|| invalid("Missing GitHub thread line"))?;
                    let start = node["startLine"]
                        .as_i64()
                        .or_else(|| node["originalStartLine"].as_i64())
                        .unwrap_or(end);
                    let old = node["diffSide"] == "LEFT";
                    Anchor::Line {
                        path,
                        start,
                        end,
                        side: if old { "old" } else { "new" }.into(),
                        commit: if old {
                            initial.base.clone()
                        } else {
                            initial.head.clone()
                        },
                    }
                };
                anchor.validate()?;
                threads.push(RemoteThread {
                    id,
                    anchor,
                    resolved: node["isResolved"] == true,
                    comments: mapped,
                });
            }
            if connection["pageInfo"]["hasNextPage"] != true {
                break;
            }
            cursor = Value::String(text(&connection["pageInfo"], "endCursor")?);
        }
        for (suffix, kind) in [
            (format!("/issues/{pr}/comments"), "conversation_comment"),
            (format!("/pulls/{pr}/reviews"), "review"),
        ] {
            for item in self.get_all(&format!("{}{}", repo.route(), suffix)).await? {
                if item["body"].as_str().is_none_or(|v| v.trim().is_empty()) {
                    continue;
                }
                let mut c = remote_rest_comment(&item, kind)?;
                if kind == "review" {
                    c.review_id = Some(c.id.clone());
                }
                threads.push(RemoteThread {
                    id: format!("{kind}:{}", c.id),
                    anchor: Anchor::General,
                    resolved: false,
                    comments: vec![c],
                });
            }
        }
        if self.get_pull_request(repo, pr).await?.head != initial.head {
            return Err(Error::Conflict("PR head changed during refresh".into()));
        }
        Ok(ForgeSnapshot {
            complete: true,
            head: initial.head,
            base: Some(initial.base),
            threads,
        })
    }

    pub async fn get_all(&self, route: &str) -> Result<Vec<Value>> {
        let mut entries = vec![];
        for page in 1..=1000 {
            let response = self
                .get(route, Some(&json!({"per_page":100,"page":page})))
                .await?;
            let items = response
                .as_array()
                .ok_or_else(|| invalid("Malformed GitHub page"))?;
            entries.extend(items.clone());
            if items.len() < 100 {
                return Ok(entries);
            }
        }
        Err(invalid(
            "GitHub pagination exceeded safety limit; snapshot not applied",
        ))
    }

    pub async fn get(&self, route: &str, params: Option<&Value>) -> Result<Value> {
        timed(self.client.get(route, params)).await
    }

    pub async fn post(&self, route: &str, body: &Value) -> Result<Value> {
        timed(self.client.post(route, Some(body))).await
    }

    pub async fn patch(&self, route: &str, body: &Value) -> Result<Value> {
        timed(self.client.patch(route, Some(body))).await
    }

    pub async fn delete(&self, route: &str) -> Result<()> {
        let response = timed(self.client._delete(
            format!("{}{}", self.api_url.trim_end_matches('/'), route),
            None::<&()>,
        ))
        .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(invalid(format!(
                "GitHub HTTP {}: deletion rejected",
                response.status()
            )))
        }
    }

    pub async fn graphql(&self, body: Value) -> Result<Value> {
        timed(self.client.graphql(&body)).await
    }

    pub async fn from_environment() -> Result<Self> {
        if let Ok(endpoint) = std::env::var("LGTM_GITHUB_TEST_API_URL") {
            if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() != Ok("1") {
                return Err(invalid("Test GitHub endpoint requires explicit test mode"));
            }
            return Self::for_test(&endpoint);
        }
        let account = std::env::var("LGTM_GITHUB_ACCOUNT").ok();
        let token = super::credentials::get_token(account.as_deref()).await?;
        let client = Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|_| invalid("Could not initialize GitHub client"))?;
        Ok(Self {
            client,
            api_url: "https://api.github.com".into(),
        })
    }

    /// Explicit HTTP fixture boundary; refuses remote hosts and never reads real credentials.
    pub fn for_test(endpoint: &str) -> Result<Self> {
        let url = url::Url::parse(endpoint).map_err(|_| invalid("Invalid GitHub test endpoint"))?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid(
                "GitHub test endpoint must be credential-free loopback HTTP",
            ));
        }
        let client = Octocrab::builder()
            .base_uri(endpoint)
            .map_err(|_| invalid("Invalid test endpoint"))?
            .personal_token("lgtm-test-fixture-only".to_owned())
            .build()
            .map_err(|_| invalid("Could not initialize test GitHub client"))?;
        Ok(Self {
            client,
            api_url: endpoint.trim_end_matches('/').into(),
        })
    }
}

async fn timed<T>(future: impl std::future::Future<Output = octocrab::Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .map_err(|_| invalid("GitHub request timed out; reconcile before retrying writes"))?
        .map_err(|e| {
            // Do not forward HTTP request bodies, tokens, or provider diagnostics.
            if let octocrab::Error::GitHub { source, .. } = &e {
                invalid(format!(
                    "GitHub HTTP {}: request rejected; check account permissions/rate limits",
                    source.status_code
                ))
            } else {
                invalid("GitHub transport failed; reconcile before retrying writes")
            }
        })
}

fn text(v: &Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("GitHub response missing {key}")))
}
fn number(v: &Value, key: &str) -> Result<i64> {
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
fn pull_request(repo: &Repository, v: &Value) -> Result<PullRequest> {
    Ok(PullRequest {
        number: number(v, "number")?,
        title: text(v, "title")?,
        head: text(&v["head"], "sha")?,
        base: text(&v["base"], "sha")?,
        repository_url: repo.repository_url.clone(),
    })
}
fn remote_author(user: &Value) -> Result<RemoteAuthor> {
    let name = text(user, "login")?;
    let account_id = opaque(
        user,
        if user.get("databaseId").is_some() {
            "databaseId"
        } else {
            "id"
        },
    )?;
    Ok(RemoteAuthor {
        name: name.clone(),
        email: format!("{account_id}+{name}@users.noreply.github.com"),
        account_id,
    })
}
fn remote_comment(v: &Value, kind: &str, reply_to: Option<String>) -> Result<RemoteComment> {
    Ok(RemoteComment {
        id: opaque(v, "databaseId")?,
        kind: kind.into(),
        review_id: v["pullRequestReview"]["databaseId"]
            .as_u64()
            .map(|i| i.to_string()),
        body: text(v, "body")?,
        updated_at: text(v, "updatedAt")?,
        author: remote_author(&v["author"])?,
        reply_to,
    })
}
fn remote_rest_comment(v: &Value, kind: &str) -> Result<RemoteComment> {
    Ok(RemoteComment {
        id: opaque(v, "id")?,
        kind: kind.into(),
        review_id: None,
        body: text(v, "body")?,
        updated_at: v["updated_at"]
            .as_str()
            .or(v["submitted_at"].as_str())
            .unwrap_or("")
            .into(),
        author: remote_author(&v["user"])?,
        reply_to: None,
    })
}
const THREADS_QUERY: &str = r#"query($owner:String!,$name:String!,$number:Int!,$cursor:String){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100,after:$cursor){nodes{id path line startLine originalLine originalStartLine diffSide subjectType isResolved comments(first:100){nodes{databaseId body updatedAt author{login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{databaseId}} pageInfo{hasNextPage endCursor}}} pageInfo{hasNextPage endCursor}}}}}"#;
const COMMENTS_QUERY: &str = r#"query($id:ID!,$cursor:String){node(id:$id){... on PullRequestReviewThread{comments(first:100,after:$cursor){nodes{databaseId body updatedAt author{login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{databaseId}} pageInfo{hasNextPage endCursor}}}}}"#;
