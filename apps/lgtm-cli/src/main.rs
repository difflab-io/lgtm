use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use lgtm_forge::error::Error as ForgeError;
use tracing::Instrument;

mod commands;
mod daemon;
mod log;
mod models;
mod rpc_client;
mod server;
mod store;

// types -----------------------------------------------------------------------

#[derive(Subcommand)]
enum Command {
    /// Start the background LGTM RPC server.
    Up(Up),
    /// Stop the background LGTM RPC server.
    Down,
    /// Show whether the background LGTM RPC server is running.
    Status,
    /// Open a workspace directory in the LGTM Desktop application.
    Open(commands::open::OpenArgs),
    /// Stage worktree paths into LGTM snapshot storage or the working checkout.
    Add(commands::add::AddArgs),
    /// Commit staged or modified work with optional agent attribution.
    Commit(commands::commit::CommitArgs),
    /// Push committed changes on the active branch to the remote repository.
    Push(commands::push::PushArgs),
    /// Manage local snapshot and remote pull request reviews.
    Review(commands::review::ReviewArgs),
    /// Create, update, resolve, or delete review comments.
    Comment(commands::comment::CommentArgs),
    /// Synchronize `.lgtm/mise.toml` and run or list LGTM-managed Mise tasks.
    Run(commands::run::RunArgs),
    #[command(hide = true)]
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
        #[arg(long)]
        daemonize: bool,
    },
}

#[derive(Args)]
struct Up {
    /// Local port for the RPC server (0 selects a free port).
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Run in this terminal instead of starting a background process.
    #[arg(long)]
    foreground: bool,
}

// Command ---------------------------------------------------------------------

/// Run LGTM code reviews, workspace tasks, and the LGTM Desktop IDE.
#[derive(Parser)]
#[command(name = "lgtm", version, about, args_conflicts_with_subcommands = true)]
struct Cli {
    /// Return machine-readable JSON, with diagnostics on stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Optional workspace directory to open in LGTM Desktop when no subcommand is given.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

// Callbacks -------------------------------------------------------------------

fn main() {
    let json_output = std::env::args().any(|arg| arg == "--json");
    if let Err(error) = run() {
        let code = error
            .downcast_ref::<connectrpc::ConnectError>()
            .map(|connect_err| connect_err.code.as_str())
            .or_else(|| {
                error
                    .downcast_ref::<ForgeError>()
                    .map(|domain_err| match domain_err {
                        ForgeError::Invalid(_) => "invalid_argument",
                        ForgeError::NotFound(_) => "not_found",
                        _ => "command_failed",
                    })
            })
            .unwrap_or("command_failed");
        if json_output {
            println!(
                "{}",
                serde_json::json!({
                    "error": {
                        "code": code,
                        "message": error.to_string(),
                    }
                })
            );
        } else {
            tracing::error!("{error:#}");
            eprintln!("{error:#}");
        }
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let store = store::Store::new()?;
    let command_choice = cli
        .command
        .unwrap_or(Command::Open(commands::open::OpenArgs { path: cli.path }));
    // Fork only in the fresh server process, before starting Tokio threads.
    if let Command::Serve {
        daemonize: true, ..
    } = command_choice
    {
        daemon::detach(&store)?;
    }
    if matches!(command_choice, Command::Serve { .. })
        || matches!(&command_choice, Command::Up(options) if options.foreground)
    {
        log::init(&store)?;
    }
    let command = match &command_choice {
        Command::Up(_) => "up",
        Command::Down => "down",
        Command::Status => "status",
        Command::Open(_) => "open",
        Command::Add(_) => "add",
        Command::Review(_) => "review",
        Command::Commit(_) => "commit",
        Command::Push(_) => "push",
        Command::Comment(_) => "comment",
        Command::Run(_) => "run",
        Command::Serve { .. } => "serve",
    };
    tokio::runtime::Runtime::new()?.block_on(
        async {
            match command_choice {
                Command::Up(options) if options.foreground => {
                    server::serve(options.port, &store, false).await
                }
                Command::Up(options) => daemon::up(options.port, &store, cli.json).await,
                Command::Down => daemon::down(&store).await,
                Command::Status => daemon::status(&store, cli.json).await,
                Command::Open(args) => commands::open::run(args, cli.json),
                Command::Add(args) => commands::add::run(args, &store, cli.json).await,
                Command::Review(args) => commands::review::run(args, &store, cli.json).await,
                Command::Commit(args) => commands::commit::run(args, &store, cli.json).await,
                Command::Push(args) => commands::push::run(args, &store, cli.json).await,
                Command::Comment(args) => commands::comment::run(args, &store, cli.json).await,
                Command::Run(args) => commands::run::run(args, cli.json).await,
                Command::Serve { port, daemonize } => server::serve(port, &store, daemonize).await,
            }
        }
        .instrument(tracing::info_span!("cli_command", command)),
    )
}
