//! Octocrab-backed GitHub REST/GraphQL API with complete paginated snapshots.
use super::{ForgeClient, models::Repository};
use crate::extensions::gh::{self, COMMENTS_QUERY, number, opaque, request, text};
use crate::{
    error::{Error, Result, invalid},
    forge::models::{ForgeSnapshot, PullRequest, RemoteAuthor, RemoteComment, RemoteThread},
    models::Anchor,
};
use octocrab::Octocrab;
use serde_json::{Value, json};

/// Cloneable provider. Test construction always uses an inert fixture token.
#[derive(Clone)]
pub struct GithubClient {
    pub(crate) client: Octocrab,
}

impl GithubClient {
    // Public ------------------------------------------------------------------

    // Construction and configuration ------------------------------------------

    pub async fn from_environment() -> Result<Self> {
        if let Ok(endpoint) = std::env::var("LGTM_GITHUB_TEST_API_URL") {
            if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() != Ok("1") {
                return Err(invalid("Test GitHub endpoint requires explicit test mode"));
            }
            return Self::for_test_actor(
                &endpoint,
                &std::env::var("LGTM_GITHUB_TEST_ACTOR").unwrap_or_default(),
            );
        }
        let account = std::env::var("LGTM_GITHUB_ACCOUNT").ok();
        let token = gh::get_token(account.as_deref()).await?;
        let client = Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|_| invalid("Could not initialize GitHub client"))?;
        Ok(Self { client })
    }

    /// Explicit HTTP fixture boundary; refuses remote hosts and never reads real credentials.
    pub fn for_test(endpoint: &str) -> Result<Self> {
        Self::for_test_actor(endpoint, "")
    }

    fn for_test_actor(endpoint: &str, actor: &str) -> Result<Self> {
        let token = match actor {
            "" => "lgtm-test-fixture-only",
            "primary" => "lgtm-fixture-primary",
            "secondary" => "lgtm-fixture-secondary",
            "invalid" => "lgtm-fixture-invalid",
            _ => return Err(invalid("Unknown GitHub fixture actor")),
        };
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
            .personal_token(token.to_owned())
            .build()
            .map_err(|_| invalid("Could not initialize test GitHub client"))?;
        Ok(Self { client })
    }

    // Transport ---------------------------------------------------------------

    pub async fn get(&self, route: &str, params: Option<&Value>) -> Result<Value> {
        request(self.client.get(route, params)).await
    }

    pub async fn post(&self, route: &str, body: &Value) -> Result<Value> {
        request(self.client.post(route, Some(body))).await
    }

    pub async fn patch(&self, route: &str, body: &Value) -> Result<Value> {
        request(self.client.patch(route, Some(body))).await
    }

    pub async fn delete(&self, route: &str) -> Result<()> {
        request(async {
            let response = self.client._delete(route, None::<&()>).await?;
            octocrab::map_github_error(response).await?;
            Ok(())
        })
        .await
    }

    pub async fn graphql(&self, body: Value) -> Result<Value> {
        request(self.client.graphql(&body)).await
    }

    // Private -----------------------------------------------------------------

    // Snapshots and response mapping ------------------------------------------

    async fn get_snapshot_once(&self, repo: &Repository, pr: i64) -> Result<ForgeSnapshot> {
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
                        Self::remote_comment(
                            c,
                            "review_comment",
                            if i > 0 { Some(root_id.clone()) } else { None },
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                let path = text(node, "path")?;
                let outdated = node["isOutdated"] == true;
                let current = if node["subjectType"] == "FILE" {
                    Some(Anchor::File { path: path.clone() })
                } else if let Some(end) = node["line"].as_i64() {
                    let old = node["diffSide"] == "LEFT";
                    Some(Anchor::Line {
                        path: path.clone(),
                        start: node["startLine"].as_i64().unwrap_or(end),
                        end,
                        side: if old { "old" } else { "new" }.into(),
                    })
                } else {
                    None
                };
                let observation = Some(crate::models::ForgeObservation {
                    base: initial.base.clone(),
                    head: initial.head.clone(),
                    outdated,
                    current,
                });
                let original_path = comments[0]["path"].as_str().unwrap_or(&path).to_owned();
                let anchor = if node["subjectType"] == "FILE" {
                    Anchor::File {
                        path: original_path,
                    }
                } else {
                    let end = node["originalLine"]
                        .as_i64()
                        .or_else(|| (!outdated).then(|| node["line"].as_i64()).flatten())
                        .ok_or_else(|| invalid("Missing GitHub thread line"))?;
                    // Original and current coordinates can differ after a push.
                    // A single-line original has no originalStartLine, even when
                    // GitHub supplies a relocated current startLine.
                    let start = if node["originalLine"].as_i64().is_some() {
                        node["originalStartLine"].as_i64().unwrap_or(end)
                    } else {
                        node["startLine"].as_i64().unwrap_or(end)
                    };
                    let old = node["diffSide"] == "LEFT";
                    Anchor::Line {
                        path: original_path,
                        start,
                        end,
                        side: if old { "old" } else { "new" }.into(),
                    }
                };
                anchor.validate()?;
                threads.push(RemoteThread {
                    id,
                    anchor,
                    observation,
                    orphan_reason: None,
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
            for item in gh::get_all(&self.client, &format!("{}{}", repo.route(), suffix)).await? {
                if kind == "review" && item["state"] == "PENDING" {
                    continue;
                }
                if item["body"].as_str().is_none_or(|v| v.trim().is_empty()) {
                    continue;
                }
                let mut c = Self::remote_rest_comment(&item, kind)?;
                if kind == "review" {
                    c.review_id = Some(c.id.clone());
                }
                threads.push(RemoteThread {
                    id: format!("{kind}:{}", c.id),
                    anchor: Anchor::General,
                    observation: None,
                    orphan_reason: None,
                    resolved: false,
                    comments: vec![c],
                });
            }
        }
        // Outdated coordinates alone cannot prove a deleted file. Read changed-file
        // metadata only for these threads; never fetch file bodies during refresh.
        if threads.iter().any(|thread| {
            thread
                .observation
                .as_ref()
                .is_some_and(|observation| observation.outdated)
        }) {
            let files =
                gh::get_all(&self.client, &format!("{}/pulls/{pr}/files", repo.route())).await?;
            for thread in &mut threads {
                let Some(observation) = &mut thread.observation else {
                    continue;
                };
                if !observation.outdated {
                    continue;
                }
                let original_path = match &thread.anchor {
                    Anchor::File { path } | Anchor::Line { path, .. } => path,
                    Anchor::General => continue,
                };
                let Some(file) = files.iter().find(|file| {
                    file["filename"].as_str() == Some(original_path.as_str())
                        || file["previous_filename"].as_str() == Some(original_path.as_str())
                }) else {
                    continue;
                };
                if file["status"] == "removed" {
                    thread.orphan_reason = Some(crate::models::OrphanReason::FileDeleted);
                    observation.current = None;
                } else {
                    let current_path = text(file, "filename")?;
                    match &mut observation.current {
                        Some(Anchor::File { path } | Anchor::Line { path, .. }) => {
                            *path = current_path
                        }
                        _ => observation.current = Some(Anchor::File { path: current_path }),
                    }
                }
            }
        }
        let latest = self.get_pull_request(repo, pr).await?;
        if latest.head != initial.head
            || latest.base != initial.base
            || latest.state != initial.state
        {
            return Err(Error::Conflict(
                "PR comparison changed during refresh".into(),
            ));
        }
        Ok(ForgeSnapshot {
            complete: true,
            state: Some(initial.state),
            head: initial.head,
            base: Some(initial.base),
            threads,
        })
    }

    fn pull_request(repo: &Repository, v: &Value) -> Result<PullRequest> {
        Ok(PullRequest {
            number: number(v, "number")?,
            state: if v["merged"] == true || v["merged_at"].is_string() {
                crate::database::types::ForgeState::Merged
            } else if v["state"] == "closed" {
                crate::database::types::ForgeState::Closed
            } else {
                crate::database::types::ForgeState::Open
            },
            title: text(v, "title")?,
            head: text(&v["head"], "sha")?,
            base: text(&v["base"], "sha")?,
            repository_url: repo.repository_url.clone(),
            author_account_id: v.get("user").map(|user| opaque(user, "id")).transpose()?,
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
            pending: v["pullRequestReview"]["state"] == "PENDING",
            id: opaque(v, "databaseId")?,
            kind: kind.into(),
            review_id: v["pullRequestReview"]["databaseId"]
                .as_u64()
                .map(|i| i.to_string()),
            body: text(v, "body")?,
            updated_at: text(v, "updatedAt")?,
            author: Self::remote_author(&v["author"])?,
            reply_to,
        })
    }

    fn remote_rest_comment(v: &Value, kind: &str) -> Result<RemoteComment> {
        Ok(RemoteComment {
            pending: v["state"] == "PENDING",
            id: opaque(v, "id")?,
            kind: kind.into(),
            review_id: None,
            body: text(v, "body")?,
            updated_at: v["updated_at"]
                .as_str()
                .or(v["submitted_at"].as_str())
                .unwrap_or("")
                .into(),
            author: Self::remote_author(&v["user"])?,
            reply_to: None,
        })
    }
}

impl ForgeClient for GithubClient {
    async fn get_pull_requests(
        &self,
        repo: &Repository,
        query: &str,
        page: u32,
    ) -> Result<(Vec<PullRequest>, u32)> {
        let page = page.max(1);
        let query = query.trim();
        // Branch selection must not depend on GitHub's asynchronously indexed search.
        let branch = query
            .strip_prefix("head:")
            .filter(|branch| !branch.is_empty() && !branch.chars().any(char::is_whitespace));
        if !query.is_empty() && branch.is_none() {
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
        let mut params = json!({"state":"open","per_page":30,"page":page});
        if let Some(branch) = branch {
            params["head"] = json!(if branch.contains(':') {
                branch.to_owned()
            } else {
                format!("{}:{branch}", repo.owner)
            });
        }
        let response = self
            .get(&format!("{}/pulls", repo.route()), Some(&params))
            .await?;
        let items = response
            .as_array()
            .ok_or_else(|| invalid("Malformed GitHub PR list"))?;
        let entries = items
            .iter()
            .map(|v| Self::pull_request(repo, v))
            .collect::<Result<Vec<_>>>()?;
        Ok((entries, if items.len() == 30 { page + 1 } else { 0 }))
    }

    async fn get_pull_request(&self, repo: &Repository, pr: i64) -> Result<PullRequest> {
        Self::pull_request(
            repo,
            &self
                .get(&format!("{}/pulls/{pr}", repo.route()), None)
                .await?,
        )
    }

    async fn get_account_id(&self) -> Result<String> {
        opaque(&self.get("/user", None).await?, "id")
    }

    /// Retry one complete observation if commits move while its pages are loading.
    /// Provider writes are never retried by this read path.
    async fn get_snapshot(&self, repo: &Repository, pr: i64) -> Result<ForgeSnapshot> {
        match self.get_snapshot_once(repo, pr).await {
            Err(Error::Conflict(_)) => self.get_snapshot_once(repo, pr).await,
            result => result,
        }
    }
}

const THREADS_QUERY: &str = r#"query($owner:String!,$name:String!,$number:Int!,$cursor:String){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100,after:$cursor){nodes{id path line startLine originalLine originalStartLine diffSide startDiffSide subjectType isResolved isOutdated comments(first:100){nodes{id databaseId path body updatedAt author{login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{databaseId state}} pageInfo{hasNextPage endCursor}}} pageInfo{hasNextPage endCursor}}}}}"#;
