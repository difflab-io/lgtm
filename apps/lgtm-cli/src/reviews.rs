//! Review commands use backend RPCs; persistence and forge calls stay in the backend.
use crate::{
    context::{ActorArgs, ContextArgs},
    rpc::Client,
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

/// Optional guards for replaying the same reviewed mutation from a shell.
#[derive(Args, Default)]
pub struct MutationArgs {
    /// Version returned by the conversation or selected object.
    #[arg(long)]
    expected_version: Option<String>,
    /// Reuse only with the exact original payload and expected version.
    #[arg(long, requires = "expected_version")]
    request_key: Option<String>,
}

#[derive(Args)]
pub struct ReviewArgs {
    #[command(flatten)]
    pub context: ContextArgs,
    #[command(flatten)]
    pub actor: ActorArgs,
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    #[arg(long)]
    pub no_open: bool,
}
#[derive(Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}
#[derive(Subcommand)]
pub enum SessionCommand {
    /// Start or resume the matching review comparison.
    Start {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Show the active/latest review, or an explicit historical review.
    Show {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
    },
    /// Publish local drafts and complete the review without a forge write.
    Complete {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        request_key: Option<String>,
    },
    /// Export concise Markdown feedback without changing review state.
    Export {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Refresh review data or record a confirmed saved-file/directory move.
    Refresh {
        #[command(flatten)]
        context: ContextArgs,
        /// Original absolute path after moving --notes to its current location.
        #[arg(long, requires = "notes")]
        moved_from: Option<PathBuf>,
        #[command(flatten)]
        mutation: MutationArgs,
    },
    /// Reconcile a frozen submission, completing only a verified pending review.
    Reconcile {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long, visible_alias = "review")]
        session: Option<String>,
    },
    /// Submit through the backend RPC.
    Submit {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        outcome: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        request_key: Option<String>,
    },
}
#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: SessionsCommand,
}
#[derive(Subcommand)]
pub enum SessionsCommand {
    List {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        completed: bool,
    },
}
#[derive(Args)]
pub struct CommentsArgs {
    #[command(subcommand)]
    pub command: CommentsCommand,
}
#[derive(Subcommand)]
pub enum CommentsCommand {
    /// Synchronize or recover a locally saved PR draft without publishing its review.
    Sync {
        comment: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    List {
        #[command(flatten)]
        context: ContextArgs,
    },
    Add {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long, requires = "file")]
        line: Option<i64>,
        #[arg(long, requires = "line")]
        end_line: Option<i64>,
        #[arg(long,requires="line",value_parser=["old","new"])]
        side: Option<String>,
        #[command(flatten)]
        guard: MutationArgs,
    },
    Reply {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
        #[command(flatten)]
        guard: MutationArgs,
    },
    Resolve {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[command(flatten)]
        guard: MutationArgs,
    },
    Edit {
        comment: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
        #[command(flatten)]
        guard: MutationArgs,
    },
    Delete {
        comment: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[command(flatten)]
        guard: MutationArgs,
    },
    Promote {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        target_session: String,
        #[arg(long, required = true)]
        comment: Vec<String>,
        #[arg(long)]
        request_key: Option<String>,
        /// Explicit target file anchor; omit with --general for a review-wide thread.
        #[arg(long, conflicts_with = "general")]
        file: Option<String>,
        #[arg(long)]
        general: bool,
    },
    Reopen {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[command(flatten)]
        guard: MutationArgs,
    },
}

/// Machine mode starts a review through the same public contract used by editor clients.
pub async fn review(args: ReviewArgs, store: &Store, json_output: bool) -> Result<()> {
    if !json_output {
        let path = location(&args.context)?;
        return crate::ui::open_review(&path, None, None, args.port, !args.no_open, store).await;
    }
    let client = client_for_actor(store, &args.actor).await?;
    print_result(&start(&client, &args.context).await?, true)
}

pub async fn session(args: SessionArgs, store: &Store, json_output: bool) -> Result<()> {
    match args.command {
        SessionCommand::Start { context, actor } => {
            let client = client_for_actor(store, &actor).await?;
            print_result(&start(&client, &context).await?, json_output)
        }
        SessionCommand::Show { context, session } => {
            let client = Client::new(store)?;
            let review = match session {
                Some(id) => {
                    client.call("GetReview", json!({"reviewId":id})).await?["review"].clone()
                }
                None => get_review(&client, &context).await?,
            };
            print_result(&json!({"review":review}), json_output)
        }
        SessionCommand::Complete {
            context,
            actor,
            body,
            request_key,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let review = get_review(&client, &context).await?;
            print_result(&client.call("CompleteReview", json!({"reviewId":review["id"], "expectedVersion":review["version"],"body":body.unwrap_or_default(),"requestKey":key(request_key)})).await?,json_output)
        }
        SessionCommand::Export {
            context,
            session,
            output,
        } => {
            let client = Client::new(store)?;
            let scope = match session {
                Some(id) => json!({"reviewId":id}),
                None => get_scope(&client, &context, None, false).await?,
            };
            let result = client.call("Export", json!({"scope":scope})).await?;
            let text = result["markdown"]
                .as_str()
                .context("Backend returned no export content")?;
            if let Some(path) = output {
                std::fs::write(&path, text)?;
                if json_output {
                    print_result(&json!({"output":path,"markdown":text}), true)?;
                }
            } else if json_output {
                print_result(&result, true)?;
            } else {
                print!("{text}");
            }
            Ok(())
        }
        SessionCommand::Refresh {
            context,
            moved_from,
            mutation,
        } => {
            let client = client_for_actor(store, &ActorArgs::default()).await?;
            let (scope, changes) = if let Some(previous) = moved_from {
                let current = location(&context)?;
                if !current.exists() {
                    bail!("The current notes location must exist");
                }
                let previous = if previous.is_absolute() {
                    previous
                } else {
                    std::env::current_dir()?.join(previous)
                };
                let scope = if current.is_dir() {
                    let mut prior = context.clone();
                    prior.notes = Some(previous.clone());
                    json!({"reviewId":get_review(&client, &prior).await?["id"]})
                } else {
                    json!({"notes":{"path":previous}})
                };
                (
                    scope,
                    json!([{"locationMoved":{"previousPath":previous,"currentPath":current}}]),
                )
            } else {
                (get_scope(&client, &context, None, false).await?, json!([]))
            };
            let version = match mutation.expected_version {
                Some(version) => json!(version),
                None => get_conversation(&client, &scope).await?["version"].clone(),
            };
            print_result(&client.call("RefreshReview",json!({"scope":scope,"sourceChanges":changes,"expectedVersion":version,"requestKey":key(mutation.request_key)})).await?,json_output)
        }
        SessionCommand::Reconcile {
            context,
            actor,
            session,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let id = match session {
                Some(id) => json!(id),
                None => get_review(&client, &context).await?["id"].clone(),
            };
            print_result(
                &client
                    .call("ReconcileSubmission", json!({"reviewId":id}))
                    .await?,
                json_output,
            )
        }
        SessionCommand::Submit {
            context,
            actor,
            outcome,
            body,
            request_key,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let review = get_review(&client, &context).await?;
            let comparison = &review["comparison"]["pullRequest"];
            if comparison.is_null() {
                bail!("Local reviews cannot be submitted to GitHub; use session complete");
            }
            print_result(&client.call("SubmitReview",json!({"reviewId":review["id"],"outcome":outcome.unwrap_or_else(||"comment".into()),"body":body.unwrap_or_default(),"requestKey":key(request_key),"expectedHead":comparison["headCommit"],"expectedBase":comparison["baseCommit"],"expectedVersion":review["version"]})).await?,json_output)
        }
    }
}

pub async fn sessions(args: SessionsArgs, store: &Store, json_output: bool) -> Result<()> {
    let SessionsCommand::List {
        path,
        repo,
        all,
        completed,
    } = args.command;
    let client = Client::new(store)?;
    let mut reviews = get_reviews(
        &client,
        None,
        if all {
            None
        } else if completed {
            Some("completed")
        } else {
            Some("active")
        },
    )
    .await?;
    if let Some(path) = path {
        let path = path.canonicalize()?.to_string_lossy().into_owned();
        reviews.retain(|review| {
            review["target"]["localGit"]["path"] == path
                || review["target"]["directory"]["path"] == path
        });
    }
    if let Some(repository) = repo {
        reviews.retain(|review| review["target"]["pullRequest"]["repositoryUrl"] == repository);
    }
    print_result(&json!({"reviews":reviews}), json_output)
}

pub async fn comments(args: CommentsArgs, store: &Store, json_output: bool) -> Result<()> {
    let result = match args.command {
        CommentsCommand::List { context } => {
            let client = Client::new(store)?;
            let scope = get_scope(&client, &context, None, false).await?;
            get_conversation(&client, &scope).await?
        }
        CommentsCommand::Add {
            context,
            actor,
            body,
            file,
            line,
            end_line,
            side,
            guard,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let scope = get_scope(&client, &context, file.as_deref(), true).await?;
            let conversation = get_conversation(&client, &scope).await?;
            let anchor = capture_anchor(
                &client,
                &scope,
                file.as_deref(),
                line,
                end_line,
                side.as_deref(),
            )
            .await?;
            client.call("AddComment",json!({"scope":scope,"expectedVersion":guard.expected_version.map(Value::String).unwrap_or_else(||conversation["version"].clone()),"requestKey":key(guard.request_key),"body":body,"anchor":anchor})).await?
        }
        CommentsCommand::Sync {
            comment,
            context,
            actor,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let review = get_review(&client, &context).await?;
            client
                .call(
                    "SyncDraftComment",
                    json!({"reviewId":review["id"],"commentId":comment}),
                )
                .await?
        }
        CommentsCommand::Reply {
            thread,
            context,
            actor,
            body,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "threads", &thread).await?;
            client.call("Reply",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"threadId":thread,"body":body})).await?
        }
        CommentsCommand::Resolve {
            thread,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "threads", &thread).await?;
            client.call("ResolveThread",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"threadId":thread,"resolved":true})).await?
        }
        CommentsCommand::Reopen {
            thread,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "threads", &thread).await?;
            client.call("ResolveThread",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"threadId":thread,"resolved":false})).await?
        }
        CommentsCommand::Edit {
            comment,
            context,
            actor,
            body,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "comments", &comment).await?;
            client.call("EditComment",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"commentId":comment,"body":body})).await?
        }
        CommentsCommand::Delete {
            comment,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "comments", &comment).await?;
            client.call("DeleteComment",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"commentId":comment})).await?
        }
        CommentsCommand::Promote {
            context,
            actor,
            target_session,
            comment,
            request_key,
            file,
            general,
        } => {
            if file.is_none() && !general {
                bail!("Promotion requires explicit --file PATH or --general target mapping");
            }
            let client = client_for_actor(store, &actor).await?;
            let mut source = get_scope(&client, &context, None, false).await?;
            if !source["reviewId"].is_null() {
                let review = client
                    .call("GetReview", json!({"reviewId":source["reviewId"]}))
                    .await?;
                let path = review["review"]["target"]["directory"]["path"]
                    .as_str()
                    .context("Promotion source must be local notes")?;
                source = json!({"notes":{"path":path}});
            }
            let source_conversation = get_conversation(&client, &source).await?;
            let target_scope = json!({"reviewId":target_session});
            let target_conversation = get_conversation(&client, &target_scope).await?;
            let anchor =
                capture_anchor(&client, &target_scope, file.as_deref(), None, None, None).await?;
            let anchors: Vec<_> = comment.iter().map(|_| anchor.clone()).collect();
            client.call("PromoteNotes",json!({"source":source,"targetReviewId":target_session,"expectedSourceVersion":source_conversation["version"],"expectedReviewVersion":target_conversation["version"],"commentIds":comment,"anchors":anchors,"requestKey":key(request_key)})).await?
        }
    };
    print_result(&result, json_output)
}

async fn object_guard(
    store: &Store,
    args: &ContextArgs,
    actor: &ActorArgs,
    guard: &MutationArgs,
    collection: &str,
    id: &str,
) -> Result<(Client, Value, Value)> {
    let client = client_for_actor(store, actor).await?;
    let scope = get_scope(&client, args, None, false).await?;
    if let Some(version) = &guard.expected_version {
        return Ok((client, scope, json!(version)));
    }
    let conversation = get_conversation(&client, &scope).await?;
    let version = conversation[collection]
        .as_array()
        .and_then(|objects| objects.iter().find(|object| object["id"] == id))
        .context("Object is outside the selected scope")?["version"]
        .clone();
    Ok((client, scope, version))
}

fn location(args: &ContextArgs) -> Result<PathBuf> {
    let supplied = args
        .notes
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
    if args.base.is_some() && args.mode != Some(crate::context::ReviewMode::Branch) {
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
                    .or_else(|| {
                        git_value(
                            &["remote", "get-url", "origin"],
                            Some(&location(args).ok()?),
                        )
                    })
                    .context("--pr NUMBER requires --repo URL or a GitHub origin")?;
                (repository, pr.parse::<i64>()?)
            };
        return Ok(
            json!({"pullRequest":{"repositoryUrl":repository_url(&repository,&args.host)?,"number":number.to_string()}}),
        );
    }
    let path = location(args)?;
    if args.notes.is_some() {
        return Ok(json!({"directory":{"path":path}}));
    }
    if let Some(root) = git_value(&["rev-parse", "--show-toplevel"], Some(&path)) {
        let branch = args
            .branch
            .clone()
            .or_else(|| git_value(&["symbolic-ref", "HEAD"], Some(&path)))
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
        bail!("File notes need no review; use comments add --notes FILE");
    }
    let comparison = if target["localGit"].is_object() {
        let root = std::path::Path::new(
            target["localGit"]["path"]
                .as_str()
                .context("Missing local repository")?,
        );
        let head = git_value(&["rev-parse", "HEAD"], Some(root))
            .context("Repository has no HEAD commit")?;
        match args.mode.unwrap_or(crate::context::ReviewMode::Working) {
            crate::context::ReviewMode::Working => json!({"working":{"baseCommit":head}}),
            crate::context::ReviewMode::Staged => json!({"staged":{"baseCommit":head}}),
            crate::context::ReviewMode::Branch => {
                let base = git_value(
                    &["merge-base", args.base.as_deref().unwrap_or("main"), "HEAD"],
                    Some(root),
                )
                .context("Cannot find a common ancestor with the selected base")?;
                json!({"branch":{"baseCommit":base,"headCommit":head}})
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
        && let Some(root) = git_value(&["rev-parse", "--show-toplevel"], Some(&location(args)?))
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
    file: Option<&str>,
    write: bool,
) -> Result<Value> {
    if args.notes.is_some() {
        let path = location(args)?;
        if !path.is_dir() {
            return Ok(json!({"notes":{"path":path}}));
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
        if let Some(file) = file {
            return Ok(json!({"notes":{"path":path.join(file)}}));
        }
        bail!("Use --notes FILE for ordinary notes, or start a directory review first");
    }
    match get_review(client, args).await {
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

async fn capture_anchor(
    client: &Client,
    scope: &Value,
    file: Option<&str>,
    first: Option<i64>,
    last: Option<i64>,
    side: Option<&str>,
) -> Result<Value> {
    use crate::backend::{
        database::types::SourceKind,
        models::{AnchorEvidence, AnchorSource},
        services::workspace,
    };
    let Some(file) = file else {
        return Ok(json!({"kind":"general"}));
    };
    let side = side.unwrap_or("new");
    let mut path = file.to_owned();
    let (source, bytes) = if let Some(notes) = scope["notes"]["path"].as_str() {
        let selected = std::path::Path::new(notes);
        let root = selected.parent().context("Notes file has no parent")?;
        path = selected
            .file_name()
            .context("Notes file has no name")?
            .to_string_lossy()
            .into_owned();
        workspace::get_source(root, &path, SourceKind::LocalFile, None)?
    } else {
        let review = client
            .call("GetReview", json!({"reviewId":scope["reviewId"]}))
            .await?["review"]
            .clone();
        let target = &review["target"];
        if let Some(root) = target["directory"]["path"].as_str() {
            workspace::get_source(
                std::path::Path::new(root),
                &path,
                SourceKind::LocalFile,
                None,
            )?
        } else if let Some(root) = target["localGit"]["path"].as_str() {
            let comparison = &review["comparison"];
            let (kind, commit) = if side == "old" {
                (
                    SourceKind::Commit,
                    comparison
                        .as_object()
                        .and_then(|c| c.values().next())
                        .and_then(|c| c["baseCommit"].as_str()),
                )
            } else if comparison["staged"].is_object() {
                (SourceKind::IndexFile, None)
            } else if comparison["working"].is_object() {
                (SourceKind::WorkingFile, None)
            } else {
                (
                    SourceKind::Commit,
                    comparison["branch"]["headCommit"].as_str(),
                )
            };
            let mut selected =
                workspace::get_source(std::path::Path::new(root), &path, kind, commit)?;
            if kind == SourceKind::WorkingFile
                && let Ok(indexed) = workspace::get_source(
                    std::path::Path::new(root),
                    &path,
                    SourceKind::IndexFile,
                    None,
                )
                && selected.1 == indexed.1
            {
                selected = indexed;
            }
            selected
        } else {
            let diff = client
                .call(
                    "GetDiff",
                    json!({"target":target,"comparison":review["comparison"]}),
                )
                .await?["diff"]
                .clone();
            let selected = client
                .call("GetFile", json!({"diff":diff,"path":path}))
                .await?;
            let value = &selected[if side == "old" { "oldSide" } else { "newSide" }];
            path = value["path"]
                .as_str()
                .context("File does not exist on the selected side")?
                .into();
            let wire = &value["source"]["commit"];
            (
                AnchorSource::Commit {
                    sha: wire["sha"]
                        .as_str()
                        .context("Missing selected commit")?
                        .into(),
                    content_hash: wire["contentHash"]
                        .as_str()
                        .context("Missing selected content hash")?
                        .into(),
                },
                value["content"].as_str().unwrap_or("").as_bytes().to_vec(),
            )
        }
    };
    let evidence = match first {
        Some(first) => {
            AnchorEvidence::capture(source.clone(), &bytes, first, last.unwrap_or(first))?
        }
        None => AnchorEvidence::capture_file(source.clone(), &bytes)?,
    };
    let source = match source {
        AnchorSource::Commit { sha, content_hash } => {
            json!({"commit":{"sha":sha,"contentHash":content_hash}})
        }
        AnchorSource::IndexFile {
            blob_oid,
            content_hash,
        } => json!({"indexFile":{"blobOid":blob_oid,"contentHash":content_hash}}),
        AnchorSource::WorkingFile { content_hash } => {
            json!({"workingFile":{"contentHash":content_hash}})
        }
        AnchorSource::LocalFile { content_hash } => {
            json!({"localFile":{"contentHash":content_hash}})
        }
    };
    let mut anchor = json!({"kind":if first.is_some(){"line"}else{"file"},"path":path,"source":source,"evidence":{"selectedText":evidence.selected_text,"contextBefore":evidence.context_before,"contextAfter":evidence.context_after}});
    if let Some(first) = first {
        anchor["start"] = json!(first.to_string());
        anchor["end"] = json!(last.unwrap_or(first).to_string());
        anchor["side"] = json!(side);
    }
    Ok(anchor)
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
        .or_else(|| git_value(&["config", "user.name"], None))
        .context("Set git user.name, LGTM_NAME, or pass --as")?;
    let input = if let Some(model) = &actor.model {
        json!({"name":name,"model":model,"provider":actor.provider})
    } else {
        json!({"name":name,"email":std::env::var("LGTM_EMAIL").ok().or_else(||git_value(&["config","user.email"],None)).context("Set git user.email or LGTM_EMAIL")?})
    };
    let value = client.call("EnsureIdentity", input).await?;
    Ok(client.with_identity(
        value["actor"]["id"]
            .as_str()
            .context("Missing identity ID")?
            .into(),
    ))
}

fn git_value(args: &[&str], path: Option<&std::path::Path>) -> Option<String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(path) = path {
        command.current_dir(path);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
}

fn print_result(value: &Value, _json_output: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
