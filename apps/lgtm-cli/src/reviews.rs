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
    /// start or resume the context's one active review.
    Start {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// show active/latest review metadata, or a historical session.
    Show {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
    },
    /// publish drafts locally and complete the review (no forge write).
    Complete {
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
    /// export current conversation text with selected review metadata.
    Export {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Refresh PR state through the backend RPC.
    Refresh {
        #[command(flatten)]
        context: ContextArgs,
    },
    /// Reconcile a frozen submission, completing only a verified pending review.
    Reconcile {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
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
    },
    Reply {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
    },
    Resolve {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    Edit {
        comment: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
        #[arg(long)]
        body: String,
    },
    Delete {
        comment: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
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
    },
}

/// Open a review context through the API without launching a browser in machine mode.
pub async fn review(args: ReviewArgs, store: &Store, json_output: bool) -> Result<()> {
    if !json_output {
        let path = args
            .context
            .notes
            .as_ref()
            .or(args.context.path.as_ref())
            .cloned()
            .unwrap_or(std::env::current_dir()?)
            .canonicalize()?;
        return crate::ui::open_review(&path, None, None, args.port, !args.no_open, store).await;
    }
    let client = client_for_actor(store, &args.actor).await?;
    let context = ensure_context(&client, &args.context).await?;
    let session = start(&client, &context, &args.context).await?;
    print_result(&session, json_output)
}

pub async fn session(args: SessionArgs, store: &Store, json_output: bool) -> Result<()> {
    match args.command {
        SessionCommand::Start { context, actor } => {
            let client = client_for_actor(store, &actor).await?;
            let value = ensure_context(&client, &context).await?;
            print_result(&start(&client, &value, &context).await?, json_output)
        }
        SessionCommand::Show { context, session } => {
            let client = Client::new(store)?;
            let id = match session {
                Some(id) => id,
                None => selected_session(&client, &get_context(&client, &context).await?).await?,
            };
            print_result(
                &client.call("GetSession", json!({"sessionId":id})).await?,
                json_output,
            )
        }
        SessionCommand::Complete {
            context,
            actor,
            outcome,
            body,
            request_key,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let value = ensure_context(&client, &context).await?;
            print_result(&client.call("CompleteSession", json!({
                "contextId": value["id"], "outcome":outcome.unwrap_or_default(),
                "body":body.unwrap_or_default(), "requestKey":request_key.unwrap_or_else(||uuid::Uuid::new_v4().to_string())
            })).await?, json_output)
        }
        SessionCommand::Export {
            context,
            session,
            output,
        } => {
            let client = Client::new(store)?;
            let id = match session {
                Some(id) => id,
                None => selected_session(&client, &get_context(&client, &context).await?).await?,
            };
            let result = client.call("Export", json!({"sessionId":id})).await?;
            let text = result["markdown"]
                .as_str()
                .context("Backend returned no export content")?;
            if let Some(path) = output {
                std::fs::write(&path, text)?;
                if json_output {
                    print_result(&json!({"output":path,"markdown":text}), true)?;
                }
            } else if json_output {
                return print_result(&result, true);
            } else {
                print!("{text}");
            }
            Ok(())
        }
        SessionCommand::Reconcile {
            context,
            actor,
            session,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let session_id = match session {
                Some(id) => id,
                None => selected_session(&client, &get_context(&client, &context).await?).await?,
            };
            print_result(
                &client
                    .call("ReconcileSubmission", json!({"sessionId":session_id}))
                    .await?,
                json_output,
            )
        }
        SessionCommand::Refresh { context } => {
            let client = Client::new(store)?;
            let value = ensure_context(&client, &context).await?;
            print_result(
                &client
                    .call("RefreshContext", json!({"contextId":value["id"]}))
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
            let value = ensure_context(&client, &context).await?;
            let id = selected_session(&client, &value).await?;
            print_result(&client.call("SubmitReview",json!({"sessionId":id,"outcome":outcome.unwrap_or_else(||"comment".into()),"body":body.unwrap_or_default(),"requestKey":request_key.unwrap_or_else(||uuid::Uuid::new_v4().to_string()),"expectedHead":value["refreshedHead"].as_str().unwrap_or("")})).await?,json_output)
        }
    }
}

pub async fn sessions(args: SessionsArgs, store: &Store, json_output: bool) -> Result<()> {
    let SessionsCommand::List {
        path,
        repo,
        all: _,
        completed,
    } = args.command;
    let client = Client::new(store)?;
    let path = path
        .map(|p| p.canonicalize())
        .transpose()?
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let result = client
        .call("ListSessions", json!({"path":path,"completed":completed}))
        .await?;
    if let Some(repository) = repo {
        let mut filtered = result.clone();
        if let Some(entries) = filtered["entries"].as_array_mut() {
            entries.retain(|entry| {
                entry["context"]["key"]["repositoryUrl"].as_str() == Some(&repository)
            });
        }
        print_result(&filtered, json_output)
    } else {
        print_result(&result, json_output)
    }
}

pub async fn comments(args: CommentsArgs, store: &Store, json_output: bool) -> Result<()> {
    let result = match args.command {
        CommentsCommand::List { context } => {
            let client = Client::new(store)?;
            let value = get_context(&client, &context).await?;
            client
                .call(
                    "GetConversation",
                    json!({"contextId":value["id"],"includeDrafts":true}),
                )
                .await?
        }
        CommentsCommand::Add {
            context,
            actor,
            body,
            file,
            line,
            end_line,
            side,
        } => {
            let client = client_for_actor(store, &actor).await?;
            let value = ensure_context(&client, &context).await?;
            let session = start(&client, &value, &context).await?;
            let mut anchor = json!({"kind":"general"});
            if let Some(file) = file {
                anchor = json!({"kind":"file","path":file});
                if let Some(line) = line {
                    let side = side.unwrap_or_else(|| "new".into());
                    let commit = session["session"][if side == "old" { "base" } else { "head" }]
                        .as_str()
                        .unwrap_or("");
                    let mut hash = String::new();
                    if commit.is_empty() {
                        let workspace = client
                            .call(
                                "GetWorkspace",
                                json!({"contextId":value["id"],"includeIgnored":true}),
                            )
                            .await?;
                        hash = workspace["files"]
                            .as_array()
                            .and_then(|files| files.iter().find(|f| f["path"] == file))
                            .and_then(|f| f["contentHash"].as_str())
                            .context("Cannot anchor a line in an unreadable file")?
                            .into();
                    }
                    anchor = json!({"kind":"line","path":file,"start":line.to_string(),"end":end_line.unwrap_or(line).to_string(),"side":side,"commit":commit,"contentHash":hash});
                }
            }
            client
                .call(
                    "AddComment",
                    json!({"sessionId":session["session"]["id"],"body":body,"anchor":anchor}),
                )
                .await?
        }
        CommentsCommand::Reply {
            thread,
            context,
            actor,
            body,
        } => {
            let (client, id) = active(store, &context, &actor).await?;
            client
                .call(
                    "Reply",
                    json!({"sessionId":id,"threadId":thread,"body":body}),
                )
                .await?
        }
        CommentsCommand::Resolve {
            thread,
            context,
            actor,
        } => {
            let (client, id) = active(store, &context, &actor).await?;
            client
                .call(
                    "ResolveThread",
                    json!({"sessionId":id,"threadId":thread,"resolved":true}),
                )
                .await?
        }
        CommentsCommand::Reopen {
            thread,
            context,
            actor,
        } => {
            let (client, id) = active(store, &context, &actor).await?;
            client
                .call(
                    "ResolveThread",
                    json!({"sessionId":id,"threadId":thread,"resolved":false}),
                )
                .await?
        }
        CommentsCommand::Edit {
            comment,
            context,
            actor,
            body,
        } => {
            let (client, id) = active(store, &context, &actor).await?;
            client
                .call(
                    "EditComment",
                    json!({"sessionId":id,"commentId":comment,"body":body}),
                )
                .await?
        }
        CommentsCommand::Delete {
            comment,
            context,
            actor,
        } => {
            let (client, id) = active(store, &context, &actor).await?;
            client
                .call("DeleteComment", json!({"sessionId":id,"commentId":comment}))
                .await?
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
            let anchor = match (file, general) {
                (Some(path), false) => json!({"kind":"file","path":path}),
                (None, true) => json!({"kind":"general"}),
                _ => bail!("Promotion requires explicit --file PATH or --general target mapping"),
            };
            let anchors: Vec<_> = comment.iter().map(|_| anchor.clone()).collect();
            let client = client_for_actor(store, &actor).await?;
            let value = ensure_context(&client, &context).await?;
            client.call("PromoteNotes",json!({"sourceContextId":value["id"],"targetSessionId":target_session,"commentIds":comment,"anchors":anchors,"requestKey":request_key.unwrap_or_else(||uuid::Uuid::new_v4().to_string())})).await?
        }
    };
    print_result(&result, json_output)
}

async fn active(
    store: &Store,
    context: &ContextArgs,
    actor: &ActorArgs,
) -> Result<(Client, String)> {
    let client = client_for_actor(store, actor).await?;
    let value = ensure_context(&client, context).await?;
    let session = start(&client, &value, context).await?;
    Ok((
        client,
        session["session"]["id"]
            .as_str()
            .context("Missing session ID")?
            .into(),
    ))
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

fn context_input(args: &ContextArgs) -> Result<Value> {
    let path = args
        .notes
        .as_ref()
        .or(args.path.as_ref())
        .cloned()
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    let key = if let Some(pr) = &args.pr {
        let (repository, number) =
            if let Some((repo, n)) = pr.trim_end_matches('/').rsplit_once("/pull/") {
                (repo.to_owned(), n.parse::<i64>()?)
            } else {
                (
                    args.repo
                        .clone()
                        .context("--pr NUMBER requires --repo URL")?,
                    pr.parse::<i64>()?,
                )
            };
        json!({"kind":"forge_backed","repositoryUrl":repository,"number":number.to_string()})
    } else if args.notes.is_some() {
        json!({"kind":"notes","path":path})
    } else {
        let git_dir = git_value(
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            Some(&path),
        );
        match git_dir {
            Some(git_dir) => {
                let branch = args
                    .branch
                    .clone()
                    .or_else(|| git_value(&["symbolic-ref", "HEAD"], Some(&path)))
                    .context("Detached HEAD requires explicit --branch or --notes")?;
                let branch = if branch.starts_with("refs/heads/") {
                    branch
                } else {
                    format!("refs/heads/{branch}")
                };
                json!({"kind":"branch","gitDir":git_dir,"branch":branch})
            }
            None if args.branch.is_none() => json!({"kind":"notes","path":path}),
            None => bail!("Branch review requires a Git repository"),
        }
    };
    Ok(json!({"key":key,"workspacePath":path}))
}

async fn ensure_context(client: &Client, args: &ContextArgs) -> Result<Value> {
    let value = client.call("EnsureContext", context_input(args)?).await?;
    Ok(value["context"].clone())
}

async fn get_context(client: &Client, args: &ContextArgs) -> Result<Value> {
    let input = context_input(args)?;
    let key = &input["key"];
    let listed = client
        .call("ListSessions", json!({"completed":true}))
        .await?;
    let entries = listed["entries"]
        .as_array()
        .context("No review session exists for this context")?;
    let entry = entries
        .iter()
        .rev()
        .find(|entry| {
            let existing = &entry["context"]["key"];
            if existing["kind"] != key["kind"] {
                return false;
            }
            let fields: &[&str] = match key["kind"].as_str() {
                Some("notes") => &["path"],
                Some("branch") => &["gitDir", "branch"],
                Some("forge_backed") => &["repositoryUrl", "number"],
                _ => return false,
            };
            fields.iter().all(|field| existing[*field] == key[*field])
        })
        .context("No review session exists for this context; run lgtm session start first")?;
    Ok(entry["context"].clone())
}

async fn start(client: &Client, context: &Value, args: &ContextArgs) -> Result<Value> {
    let path = args.path.as_deref().unwrap_or(std::path::Path::new("."));
    let head = if context["key"]["kind"] == "branch" {
        git_value(
            &["rev-parse", args.branch.as_deref().unwrap_or("HEAD")],
            Some(path),
        )
        .unwrap_or_default()
    } else {
        context["refreshedHead"].as_str().unwrap_or("").into()
    };
    client.call("StartSession",json!({"contextId":context["id"],"base":context["refreshedBase"].as_str().unwrap_or(""),"head":head})).await
}

async fn selected_session(client: &Client, context: &Value) -> Result<String> {
    let value = client
        .call(
            "ListSessions",
            json!({"path":context["paths"][0].as_str().unwrap_or(""),"completed":true}),
        )
        .await?;
    let entries = value["entries"].as_array().context("No sessions")?;
    let entry = entries
        .iter()
        .filter(|v| v["context"]["id"] == context["id"])
        .rev()
        .find(|v| v["session"]["state"] == "active")
        .or_else(|| {
            entries
                .iter()
                .rev()
                .find(|v| v["context"]["id"] == context["id"])
        })
        .context("No session exists; run lgtm session start first")?;
    Ok(entry["session"]["id"]
        .as_str()
        .context("Missing session ID")?
        .into())
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
