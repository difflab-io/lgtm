use crate::database::Database;
use crate::{
    backend::models::ForgeSnapshot,
    backend::{
        models::{self, *},
        services::*,
    },
    context::{ActorArgs, ContextArgs, GitContextProvider, LocalGit, Selected},
    store::Store,
};
use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use std::path::PathBuf;

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
    /// Start or resume the context's one active review.
    Start {
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
    /// Show active/latest review metadata, or a historical session.
    Show {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
    },
    /// Publish drafts locally and complete the review (no forge write).
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
    /// Export current conversation text with selected review metadata.
    Export {
        #[command(flatten)]
        context: ContextArgs,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Refresh PR state through an explicitly configured fixture stub.
    Refresh {
        #[command(flatten)]
        context: ContextArgs,
    },
    /// Stub submission: complete locally with unsubmitted status.
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
    Reopen {
        thread: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        actor: ActorArgs,
    },
}

pub async fn database(store: &Store) -> Result<ReviewService> {
    let url = if let Some(url) = std::env::var_os("LGTM_DATABASE_URL").filter(|v| !v.is_empty()) {
        url.to_str()
            .context("ReviewService URL is not UTF-8")?
            .to_string()
    } else {
        store.ensure_exists()?;
        format!(
            "sqlite://{}?mode=rwc",
            store
                .path("reviews.sqlite")
                .to_str()
                .context("ReviewService path is not UTF-8")?
        )
    };
    let client = Database::connect(&url).await?;
    let auth = AuthService::new(client.clone());
    Ok(ReviewService::new(client, auth))
}
async fn context(
    db: &ReviewService,
    args: &ContextArgs,
    create: bool,
) -> Result<(Selected, models::Context)> {
    let selected = args.select(&LocalGit)?;
    let context = if create {
        db.ensure_context(
            selected.key.clone(),
            Some(selected.path.to_str().context("Invalid UTF-8 path")?),
        )
        .await?
    } else {
        db.context(&selected.key)
            .await?
            .context("No review context exists; run lgtm session start or add a comment")?
    };
    Ok((selected, context))
}
async fn refresh(db: &ReviewService, context: &models::Context) -> Result<models::Context> {
    let fixture=std::env::var_os("LGTM_FORGE_FIXTURE").context("Live forge adapter is deferred; PR start requires an explicit LGTM_FORGE_FIXTURE JSON stub (no network actions)")?;
    let snapshot: ForgeSnapshot = serde_json::from_slice(&std::fs::read(fixture)?)?;
    eprintln!("STUB: using a forge fixture; no live fetch or remote submission occurred");
    match db.apply_remote(context, snapshot, true).await {
        Ok(current) => Ok(current),
        Err(error) => {
            db.record_refresh_failure(context, &error.to_string())
                .await?;
            Err(error.into())
        }
    }
}
async fn start(
    db: &ReviewService,
    context: &models::Context,
    actor: &Actor,
    selected: &Selected,
) -> Result<Session> {
    let (context, commits) = if context.key.kind() == "pr" {
        let c = refresh(db, context).await?;
        let commits = CommitContext {
            head: c.refreshed_head.clone(),
            base: c.refreshed_base.clone(),
        };
        (c, commits)
    } else {
        (context.clone(), selected.commits.clone())
    };
    let s = db.start_session(&context, actor, commits).await?;
    eprintln!(
        "Context: {} | actor: {}",
        context.key.label(),
        actor.label()
    );
    Ok(s)
}
fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
pub async fn session(args: SessionArgs, store: &Store) -> Result<()> {
    let db = database(store).await?;
    match args.command {
        SessionCommand::Start {
            context: args,
            actor,
        } => {
            let (selected, c) = context(&db, &args, true).await?;
            let actor = actor.resolve(db.auth(), &selected.path, &LocalGit).await?;
            print_json(&start(&db, &c, &actor, &selected).await?)?;
        }
        SessionCommand::Show {
            context: args,
            session,
        } => {
            let (_, c) = context(&db, &args, false).await?;
            print_json(
                &db.selected_session(&c.id, session.as_deref(), false)
                    .await?,
            )?;
        }
        SessionCommand::Export {
            context: args,
            session,
            output,
        } => {
            let (_, c) = context(&db, &args, false).await?;
            let s = db
                .selected_session(&c.id, session.as_deref(), false)
                .await?;
            let text =
                ReviewService::export(&db.conversation(&c.id, s.state == "active").await?, &s);
            if let Some(path) = output {
                std::fs::write(path, text)?;
            } else {
                print!("{text}");
            }
        }
        SessionCommand::Refresh { context: args } => {
            let (_, c) = context(&db, &args, false).await?;
            print_json(&refresh(&db, &c).await?)?;
        }
        SessionCommand::Complete {
            context: args,
            actor,
            outcome,
            body,
            request_key,
        } => {
            finish(&db, args, actor, outcome, body, request_key, false).await?;
        }
        SessionCommand::Submit {
            context: args,
            actor,
            outcome,
            body,
            request_key,
        } => {
            finish(&db, args, actor, outcome, body, request_key, true).await?;
        }
    }
    db.database().close().await;
    Ok(())
}
async fn finish(
    db: &ReviewService,
    args: ContextArgs,
    actor: ActorArgs,
    outcome: Option<String>,
    body: Option<String>,
    key: Option<String>,
    submit: bool,
) -> Result<()> {
    let (selected, c) = context(db, &args, false).await?;
    let actor = actor.resolve(db.auth(), &selected.path, &LocalGit).await?;
    if submit {
        if c.key.kind() != "pr" {
            bail!("Submit is PR-only");
        }
        eprintln!("STUB: comments will be published locally, not posted to a forge");
    }
    let key = key.unwrap_or_else(models::id);
    let s = db
        .complete(
            &c.id,
            &actor,
            Completion {
                outcome,
                body,
                submit,
            },
            &key,
        )
        .await?;
    eprintln!(
        "Context: {} | actor: {} | retry key: {key}",
        c.key.label(),
        actor.label()
    );
    print_json(&s)
}
pub async fn sessions(args: SessionsArgs, store: &Store) -> Result<()> {
    let db = database(store).await?;
    let SessionsCommand::List {
        path,
        repo,
        all,
        completed,
    } = args.command;
    let cwd = std::env::current_dir()?.canonicalize()?;
    let inferred_repository = if all {
        None
    } else if let Some(remote) = LocalGit.output(&cwd, &["remote", "get-url", "origin"]) {
        Some(crate::context::repository_name(&remote, "github.com")?)
    } else {
        LocalGit
            .output(
                &cwd,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .map(|dir| {
                format!(
                    "local:{}",
                    PathBuf::from(dir)
                        .canonicalize()
                        .unwrap_or_else(|_| cwd.clone())
                        .display()
                )
            })
    };
    let path = if let Some(path) = path {
        Some(
            path.canonicalize()?
                .to_str()
                .context("Invalid path")?
                .to_string(),
        )
    } else if all {
        None
    } else {
        Some(cwd.to_str().context("Invalid cwd")?.into())
    };
    let repository = if let Some(repo) = repo {
        Some(crate::context::repository_name(&repo, "github.com")?)
    } else {
        inferred_repository
    };
    print_json(
        &db.list_sessions(&SessionFilter {
            path,
            repository,
            completed,
        })
        .await?,
    )?;
    db.database().close().await;
    Ok(())
}
pub async fn comments(args: CommentsArgs, store: &Store) -> Result<()> {
    let db = database(store).await?;
    match args.command {
        CommentsCommand::List { context: args } => {
            let (_, c) = context(&db, &args, false).await?;
            let live = db.conversation(&c.id, true).await?;
            print_json(&live)?;
        }
        CommentsCommand::Add {
            context: args,
            actor,
            body,
            file,
            line,
            end_line,
            side,
        } => {
            let (selected, c) = context(&db, &args, true).await?;
            let actor = actor.resolve(db.auth(), &selected.path, &LocalGit).await?;
            let s = start(&db, &c, &actor, &selected).await?;
            let anchor = if let Some(path) = file {
                if let Some(start) = line {
                    let side = side.context("Line anchors require --side old|new")?;
                    let commit = if side == "old" {
                        s.commits.base.clone()
                    } else {
                        s.commits.head.clone()
                    }
                    .context("Reviewed commit is unavailable for this line anchor")?;
                    Anchor::Line {
                        path,
                        start,
                        end: end_line.unwrap_or(start),
                        side,
                        commit,
                    }
                } else {
                    Anchor::File { path }
                }
            } else {
                Anchor::General
            };
            print_json(&db.add_comment(&s.id, &actor, &body, anchor).await?)?;
        }
        CommentsCommand::Reply {
            thread,
            context: args,
            actor,
            body,
        } => {
            let (selected, c) = context(&db, &args, false).await?;
            let actor = actor.resolve(db.auth(), &selected.path, &LocalGit).await?;
            let s = db.selected_session(&c.id, None, true).await?;
            print_json(&db.reply(&s.id, &thread, &actor, &body).await?)?;
        }
        CommentsCommand::Resolve {
            thread,
            context: args,
            actor,
        } => {
            resolve(&db, args, actor, thread, true).await?;
        }
        CommentsCommand::Reopen {
            thread,
            context: args,
            actor,
        } => {
            resolve(&db, args, actor, thread, false).await?;
        }
    }
    db.database().close().await;
    Ok(())
}
async fn resolve(
    db: &ReviewService,
    args: ContextArgs,
    actor: ActorArgs,
    thread: String,
    resolved: bool,
) -> Result<()> {
    let (selected, c) = context(db, &args, false).await?;
    let actor = actor.resolve(db.auth(), &selected.path, &LocalGit).await?;
    let s = db.selected_session(&c.id, None, true).await?;
    print_json(&db.resolve(&s.id, &thread, &actor, resolved).await?)
}
pub async fn review(args: ReviewArgs, store: &Store) -> Result<()> {
    let db = database(store).await?;
    let (selected, c) = context(&db, &args.context, true).await?;
    let actor = args
        .actor
        .resolve(db.auth(), &selected.path, &LocalGit)
        .await?;
    let s = start(&db, &c, &actor, &selected).await?;
    db.database().close().await;
    crate::ui::open_review(
        &selected.path,
        selected.repository.as_deref(),
        Some(&s.id),
        args.port,
        !args.no_open,
        store,
    )
    .await
}
