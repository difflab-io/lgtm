//! Shared target selection, identity and output helpers used by CLI command groups.
pub mod comments;
pub mod review;
pub mod session;

use crate::{
    models::{ActorArgs, ContextArgs},
    rpc_client::Client,
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use gix::bstr::ByteSlice;
use serde_json::{Value, json};
use std::path::PathBuf;

// Private ---------------------------------------------------------------------

fn location(args: &ContextArgs) -> Result<PathBuf> {
    let supplied = args
        .directory
        .as_ref()
        .or(args.path.as_ref())
        .cloned()
        .unwrap_or(std::env::current_dir()?);
    let path = if supplied.is_absolute() {
        supplied
    } else {
        std::env::current_dir()?.join(supplied)
    };
    Ok(path.canonicalize().unwrap_or(path))
}

fn target_input(args: &ContextArgs) -> Result<Value> {
    if args.base.is_some() && args.mode != Some(crate::models::ReviewMode::Branch) {
        bail!("--base is only valid with --mode branch");
    }
    if let Some(pr) = &args.pr {
        let (repository, number) =
            if let Some((repository, number)) = pr.trim_end_matches('/').rsplit_once("/pull/") {
                (repository.to_owned(), number.parse::<i64>()?)
            } else {
                let repository = args
                    .repo
                    .clone()
                    .or_else(|| get_git_config("remote.origin.url", Some(&location(args).ok()?)))
                    .context("--pr NUMBER requires --repo URL or a GitHub origin")?;
                (repository, pr.parse::<i64>()?)
            };
        return Ok(
            json!({"pullRequest":{"repositoryUrl":repository_url(&repository,&args.host)?,"number":number.to_string()}}),
        );
    }
    let path = location(args)?;
    if args.directory.is_some() {
        return Ok(json!({"directory":{"path":path}}));
    }
    if let Ok(repo) = gix::discover(&path)
        && let Some(root) = repo.workdir()
    {
        let branch = args
            .branch
            .clone()
            .or_else(|| {
                repo.head_name()
                    .ok()
                    .flatten()
                    .and_then(|name| name.as_bstr().to_str().ok().map(str::to_owned))
            })
            .context("Check out a branch for local Git review")?;
        let branch = if branch.starts_with("refs/heads/") {
            branch
        } else {
            format!("refs/heads/{branch}")
        };
        Ok(json!({"localGit":{"path":root,"branch":branch}}))
    } else if args.branch.is_some() {
        bail!("Branch review requires a Git repository")
    } else {
        Ok(json!({"directory":{"path":path}}))
    }
}

fn repository_url(input: &str, host: &str) -> Result<String> {
    let path = input.trim().trim_end_matches('/').trim_end_matches(".git");
    if path.starts_with("https://") {
        return Ok(path.into());
    }
    let prefix = format!("git@{host}:");
    let ssh = format!("ssh://git@{host}/");
    let repository = path
        .strip_prefix(&prefix)
        .or_else(|| path.strip_prefix(&ssh))
        .unwrap_or(path);
    if repository.split('/').count() != 2 {
        bail!("Use a complete repository URL or owner/repository");
    }
    Ok(format!("https://{host}/{repository}"))
}

async fn start(client: &Client, args: &ContextArgs) -> Result<Value> {
    let target = target_input(args)?;
    if target["directory"].is_object() && !location(args)?.is_dir() {
        bail!("Start a directory review with --directory PATH");
    }
    let comparison = if target["localGit"].is_object() {
        let root = std::path::Path::new(
            target["localGit"]["path"]
                .as_str()
                .context("Missing local repository")?,
        );
        let repo = gix::discover(root)?;
        let head = repo
            .head_id()
            .context("Repository has no HEAD commit")?
            .detach();
        match args.mode.unwrap_or(crate::models::ReviewMode::Working) {
            crate::models::ReviewMode::Working => {
                json!({"working":{"baseCommit":head.to_string()}})
            }
            crate::models::ReviewMode::Staged => json!({"staged":{"baseCommit":head.to_string()}}),
            crate::models::ReviewMode::Branch => {
                let selected = repo.rev_parse_single(
                    args.base.as_deref().unwrap_or("main").as_bytes().as_bstr(),
                )?;
                let base = repo
                    .merge_base(selected.detach(), head)?
                    .context("Cannot find a common ancestor with the selected base")?
                    .to_string();
                json!({"branch":{"baseCommit":base,"headCommit":head.to_string()}})
            }
        }
    } else {
        Value::Null
    };
    let mut input = json!({"target":target,"requestKey":key(None),"resumePolicy":"RESUME_POLICY_MATCHING_ONLY"});
    if !comparison.is_null() {
        input["comparison"] = comparison;
    }
    if target["pullRequest"].is_object()
        && let Ok(repo) = gix::discover(location(args)?)
        && let Some(root) = repo.workdir()
    {
        input["localGitPath"] = json!(root);
    }
    client.call("StartReview", input).await
}

async fn get_reviews(
    client: &Client,
    target: Option<&Value>,
    state: Option<&str>,
) -> Result<Vec<Value>> {
    let mut reviews = Vec::new();
    let mut token = String::new();
    loop {
        let mut input = json!({"pageToken":token,"pageSize":200});
        if let Some(target) = target {
            input["target"] = target.clone();
        }
        if let Some(state) = state {
            input["state"] = json!(state);
        }
        let page = client.call("ListReviews", input).await?;
        reviews.extend(page["reviews"].as_array().into_iter().flatten().cloned());
        token = page["nextPageToken"].as_str().unwrap_or("").into();
        if token.is_empty() {
            break;
        }
    }
    Ok(reviews)
}

async fn get_review(client: &Client, args: &ContextArgs) -> Result<Value> {
    let target = target_input(args)?;
    get_reviews(client, Some(&target), None)
        .await?
        .into_iter()
        .max_by_key(|review| {
            (
                review["state"] == "active",
                review["createdAt"].as_str().unwrap_or("").to_owned(),
            )
        })
        .context("No saved review for this target; run lgtm session start first")
}

async fn get_scope(
    client: &Client,
    args: &ContextArgs,
    _file: Option<&str>,
    write: bool,
) -> Result<Value> {
    if args.directory.is_some() {
        let path = location(args)?;
        if !path.is_dir() {
            bail!("--directory requires a directory; live-buffer notes are no longer supported");
        }
        let target = target_input(args)?;
        if let Some(review) = get_reviews(client, Some(&target), None)
            .await?
            .into_iter()
            .max_by_key(|review| {
                (
                    review["state"] == "active",
                    review["createdAt"].as_str().unwrap_or("").to_owned(),
                )
            })
        {
            return Ok(json!({"reviewId":review["id"]}));
        }
        bail!("Start a directory review before adding feedback");
    }
    match get_review(client, args).await {
        Ok(review)
            if write
                && args.pr.is_some()
                && review["state"] == "completed"
                && review["submissionStatus"] == "confirmed" =>
        {
            Ok(json!({"reviewId":start(client, args).await?["review"]["id"]}))
        }
        Ok(review) => Ok(json!({"reviewId":review["id"]})),
        Err(error) if !write => Err(error),
        Err(_) => Ok(json!({"reviewId":start(client,args).await?["review"]["id"]})),
    }
}

async fn get_conversation(client: &Client, scope: &Value) -> Result<Value> {
    let mut result = json!({"threads":[],"comments":[]});
    let mut token = String::new();
    loop {
        let page = client
            .call(
                "GetConversation",
                json!({"scope":scope,"includeDrafts":true,"pageToken":token,"pageSize":500}),
            )
            .await?;
        if !result["version"].is_null() && result["version"] != page["version"] {
            bail!("Conversation changed while loading; try again");
        }
        result["version"] = page["version"].clone();
        for thread in page["threads"].as_array().into_iter().flatten() {
            if !result["threads"]
                .as_array()
                .unwrap()
                .iter()
                .any(|existing| existing["id"] == thread["id"])
            {
                result["threads"]
                    .as_array_mut()
                    .unwrap()
                    .push(thread.clone());
            }
        }
        result["comments"]
            .as_array_mut()
            .unwrap()
            .extend(page["comments"].as_array().into_iter().flatten().cloned());
        token = page["nextPageToken"].as_str().unwrap_or("").into();
        if token.is_empty() {
            break;
        }
    }
    Ok(result)
}

fn key(supplied: Option<String>) -> String {
    supplied.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

async fn client_for_actor(store: &Store, actor: &ActorArgs) -> Result<Client> {
    let client = Client::new(store)?;
    if let Some(id) = &actor.identity {
        return Ok(client.with_identity(id.clone()));
    }
    let name = actor
        .as_name
        .clone()
        .or_else(|| std::env::var("LGTM_NAME").ok())
        .or_else(|| get_git_config("user.name", None))
        .context("Set git user.name, LGTM_NAME, or pass --as")?;
    let input = if let Some(model) = &actor.model {
        json!({"name":name,"model":model,"provider":actor.provider})
    } else {
        json!({"name":name,"email":std::env::var("LGTM_EMAIL").ok().or_else(||get_git_config("user.email",None)).context("Set git user.email or LGTM_EMAIL")?})
    };
    let value = client.call("EnsureIdentity", input).await?;
    Ok(client.with_identity(
        value["actor"]["id"]
            .as_str()
            .context("Missing identity ID")?
            .into(),
    ))
}

fn get_git_config(key: &str, path: Option<&std::path::Path>) -> Option<String> {
    let root = path
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let value = if let Ok(repo) = gix::discover(&root) {
        repo.config_snapshot().string(key)?
    } else {
        gix::config::File::from_globals().ok()?.string(key)?
    };
    let value = value.to_str().ok()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn print_result(value: &Value, _json_output: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
