//! Comments command arguments and RPC callbacks.
use super::{client_for_actor, get_conversation, get_review, get_scope, key, print_result};
use crate::{
    models::{ActorArgs, ContextArgs, MutationArgs},
    rpc_client::Client,
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

// Types -----------------------------------------------------------------------

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

// Command ---------------------------------------------------------------------

#[derive(Args)]
pub struct CommentsArgs {
    #[command(subcommand)]
    pub command: CommentsCommand,
}

// Callbacks -------------------------------------------------------------------

pub async fn run(args: CommentsArgs, store: &Store, json_output: bool) -> Result<()> {
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
                object_guard(store, &context, &actor, &guard, "threads", &thread, true).await?;
            client.call("Reply",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"threadId":thread,"body":body})).await?
        }
        CommentsCommand::Resolve {
            thread,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "threads", &thread, false).await?;
            client.call("ResolveThread",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"threadId":thread,"resolved":true})).await?
        }
        CommentsCommand::Reopen {
            thread,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "threads", &thread, false).await?;
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
                object_guard(store, &context, &actor, &guard, "comments", &comment, false).await?;
            client.call("EditComment",json!({"scope":scope,"expectedVersion":version,"requestKey":key(guard.request_key),"commentId":comment,"body":body})).await?
        }
        CommentsCommand::Delete {
            comment,
            context,
            actor,
            guard,
        } => {
            let (client, scope, version) =
                object_guard(store, &context, &actor, &guard, "comments", &comment, false).await?;
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

// Private ---------------------------------------------------------------------

async fn object_guard(
    store: &Store,
    args: &ContextArgs,
    actor: &ActorArgs,
    guard: &MutationArgs,
    collection: &str,
    id: &str,
    starts_draft: bool,
) -> Result<(Client, Value, Value)> {
    let client = client_for_actor(store, actor).await?;
    let scope = get_scope(&client, args, None, starts_draft).await?;
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
