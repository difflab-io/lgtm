//! `lgtm` — command-line companion for the lgtm review PoC. Designed to be
//! driven by humans or by agents (`lgtm comment add` is the key verb). All
//! commands talk to a running `lgtm-grpc` server; no direct SQLite access.
//!
//! Defaults:
//! - `--backend` falls back to `$LGTM_ADDR` or `http://127.0.0.1:50051`.
//! - `--as <name>` falls back to `$LGTM_AUTHOR` or `$USER`.
//! - `--as-kind` falls back to `$LGTM_AUTHOR_KIND` or `agent` (CLI's default
//!   assumption: an agent is driving).

mod client;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lgtm_grpc::proto::*;

use crate::client::LgtmClient;

#[derive(Parser)]
#[command(name = "lgtm", version, about = "Agent-friendly CLI for the lgtm review service")]
struct Cli {
    /// Base URL of the lgtm-grpc server. Env: LGTM_ADDR.
    #[arg(
        long,
        global = true,
        env = "LGTM_ADDR",
        default_value = "http://127.0.0.1:50051"
    )]
    backend: String,

    /// Author name used when a command creates a comment. Env: LGTM_AUTHOR.
    #[arg(long = "as", global = true, env = "LGTM_AUTHOR")]
    author: Option<String>,

    /// Author kind: `user` or `agent`. Env: LGTM_AUTHOR_KIND. Default: agent.
    #[arg(
        long = "as-kind",
        global = true,
        env = "LGTM_AUTHOR_KIND",
        default_value = "agent"
    )]
    author_kind: AuthorKindArg,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum AuthorKindArg {
    User,
    Agent,
}

impl From<AuthorKindArg> for AuthorKind {
    fn from(a: AuthorKindArg) -> Self {
        match a {
            AuthorKindArg::User => AuthorKind::User,
            AuthorKindArg::Agent => AuthorKind::Agent,
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// List files changed between a base ref and the current worktree.
    Files {
        /// Path to the repo (worktree root).
        repo: PathBuf,
        /// Base ref (branch, tag, SHA). Default: HEAD.
        #[arg(long, default_value = "HEAD")]
        base: String,
    },
    /// Print the unified diff for one file.
    Diff {
        repo: PathBuf,
        #[arg(long)]
        path: String,
        #[arg(long, default_value = "HEAD")]
        base: String,
    },
    /// Create or look up a user by (name, kind).
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// List comment threads scoped to (repo, base).
    Threads {
        repo: PathBuf,
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Optional file-path filter.
        #[arg(long)]
        path: Option<String>,
        /// Hide resolved threads.
        #[arg(long)]
        unresolved: bool,
        /// Output as JSON (default: human table).
        #[arg(long)]
        json: bool,
    },
    /// Comment on a diff.
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },
}

#[derive(Subcommand)]
enum UserAction {
    /// Create or re-fetch a user; prints id.
    Upsert {
        name: String,
        #[arg(long, value_enum, default_value = "agent")]
        kind: AuthorKindArg,
    },
}

#[derive(Subcommand)]
enum CommentAction {
    /// Create a new thread with an initial comment.
    Add {
        repo: PathBuf,
        #[arg(long)]
        path: String,
        /// Line number (1-indexed, in the new file). 0 means whole-file.
        #[arg(long, default_value_t = 0)]
        line: i32,
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Body text.
        #[arg(long)]
        body: String,
    },
    /// Append a reply to an existing thread.
    Reply {
        /// Thread id (from `lgtm threads` or `lgtm comment add`).
        #[arg(long)]
        thread: i64,
        #[arg(long)]
        body: String,
    },
    /// Mark a thread resolved.
    Resolve {
        #[arg(long)]
        thread: i64,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let client = LgtmClient::new(cli.backend);

    match cli.cmd {
        Cmd::Files { repo, base } => cmd_files(&client, &repo, &base),
        Cmd::Diff { repo, path, base } => cmd_diff(&client, &repo, &path, &base),
        Cmd::User {
            action: UserAction::Upsert { name, kind },
        } => cmd_user_upsert(&client, &name, kind.into()),
        Cmd::Threads {
            repo,
            base,
            path,
            unresolved,
            json,
        } => cmd_threads(&client, &repo, &base, path.as_deref(), unresolved, json),
        Cmd::Comment { action } => cmd_comment(
            &client,
            action,
            cli.author.as_deref(),
            cli.author_kind.into(),
        ),
    }
}

// -------- commands --------

fn cmd_files(client: &LgtmClient, repo: &PathBuf, base: &str) -> Result<()> {
    let res = client.get_changed_files(&GetChangedFilesRequest {
        repo_path: abs(repo)?,
        base_ref: base.into(),
    })?;
    for f in res.files {
        println!("{:<10} {}", f.status, f.path);
    }
    Ok(())
}

fn cmd_diff(client: &LgtmClient, repo: &PathBuf, path: &str, base: &str) -> Result<()> {
    let res = client.get_file_diff(&GetFileDiffRequest {
        repo_path: abs(repo)?,
        path: path.into(),
        base_ref: base.into(),
    })?;
    print!("{}", res.diff_text);
    Ok(())
}

fn cmd_user_upsert(client: &LgtmClient, name: &str, kind: AuthorKind) -> Result<()> {
    let res = client.upsert_user(&UpsertUserRequest {
        name: name.into(),
        kind,
    })?;
    println!("{}", res.user.id);
    Ok(())
}

fn cmd_threads(
    client: &LgtmClient,
    repo: &PathBuf,
    base: &str,
    path: Option<&str>,
    unresolved: bool,
    json: bool,
) -> Result<()> {
    let res = client.list_threads(&ListThreadsRequest {
        repo_path: abs(repo)?,
        base_ref: base.into(),
        file_path: path.unwrap_or("").into(),
        only_unresolved: unresolved,
    })?;
    if json {
        serde_json::to_writer_pretty(std::io::stdout(), &res)?;
        println!();
        return Ok(());
    }
    for t in res.threads {
        let state = if t.resolved { "resolved" } else { "open" };
        println!(
            "#{id} [{state}] {file}:{line} ({n} comments)",
            id = t.id,
            file = t.file_path,
            line = t.line,
            n = t.comments.len()
        );
        for c in t.comments {
            println!("    {}: {}", c.author.name, c.body);
        }
    }
    Ok(())
}

fn cmd_comment(
    client: &LgtmClient,
    action: CommentAction,
    author: Option<&str>,
    kind: AuthorKind,
) -> Result<()> {
    match action {
        CommentAction::Add { repo, path, line, base, body } => {
            let author_id = resolve_author(client, author, kind)?;
            let res = client.create_thread(&CreateThreadRequest {
                repo_path: abs(&repo)?,
                base_ref: base,
                file_path: path,
                line,
                author_id,
                body,
            })?;
            println!("{}", res.thread.id);
        }
        CommentAction::Reply { thread, body } => {
            let author_id = resolve_author(client, author, kind)?;
            let res = client.append_comment(&AppendCommentRequest {
                thread_id: thread,
                author_id,
                body,
            })?;
            println!("{}", res.comment.id);
        }
        CommentAction::Resolve { thread } => {
            let res = client.resolve_thread(&ResolveThreadRequest { thread_id: thread })?;
            println!("thread {} resolved", res.thread.id);
        }
    }
    Ok(())
}

// -------- helpers --------

/// Resolve an author identity through the server. Falls back to $USER when
/// --as is omitted, so an interactive invocation still has an identity.
fn resolve_author(
    client: &LgtmClient,
    name: Option<&str>,
    kind: AuthorKind,
) -> Result<i64> {
    let name = name
        .map(str::to_owned)
        .or_else(|| std::env::var("USER").ok())
        .filter(|s| !s.trim().is_empty())
        .context("author name required: pass --as, or set LGTM_AUTHOR / USER")?;
    let res = client.upsert_user(&UpsertUserRequest { name, kind })?;
    Ok(res.user.id)
}

fn abs(p: &PathBuf) -> Result<String> {
    let canonical = std::fs::canonicalize(p)
        .with_context(|| format!("canonicalize {}", p.display()))?;
    Ok(canonical.to_string_lossy().into_owned())
}
