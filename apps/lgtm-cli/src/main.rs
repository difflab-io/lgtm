use clap::{Args, Parser, Subcommand};
use tracing::Instrument;

#[path = "../bundle/backend/mod.rs"]
pub mod backend;
mod context;
mod daemon;
mod log;

mod reviews;
mod rpc;
mod server;
mod store;
mod ui;

// types -----------------------------------------------------------------------

/// Run LGTM with a bundled web UI and gRPC server.
#[derive(Parser)]
#[command(name = "lgtm", version, about)]
struct Cli {
    /// Return machine-readable JSON, with diagnostics on stderr.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the UI and gRPC server in the background.
    Up(Up),
    /// Stop the background server.
    Down,
    /// Show whether the server is running.
    Status,
    /// Open the current directory in the web interface.
    Review(reviews::ReviewArgs),
    /// Scaffold for future backend session RPC commands.
    Session(reviews::SessionArgs),
    /// Scaffold for future backend session listing.
    Sessions(reviews::SessionsArgs),
    /// Scaffold for future backend comment RPC commands.
    Comments(reviews::CommentsArgs),
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
    /// Local port for both the UI and RPC API (0 selects a free port).
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Run in this terminal instead of starting a background process.
    #[arg(long)]
    foreground: bool,
    /// Start without opening a browser.
    #[arg(long)]
    no_open: bool,
}

// api -------------------------------------------------------------------------

fn main() {
    let json_output = std::env::args().any(|arg| arg == "--json");
    if let Err(error) = run() {
        if json_output {
            println!(
                "{}",
                serde_json::json!({"error":{"code":"command_failed","message":error.to_string()}})
            );
        } else {
            eprintln!("{error:#}");
        }
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let store = store::Store::new()?;
    // Fork only in the fresh server process, before starting Tokio threads.
    if let Command::Serve {
        daemonize: true, ..
    } = cli.command
    {
        daemon::detach(&store)?;
    }
    if matches!(cli.command, Command::Serve { .. })
        || matches!(&cli.command, Command::Up(options) if options.foreground)
    {
        log::init(&store)?;
    }
    let command = match &cli.command {
        Command::Up(_) => "up",
        Command::Down => "down",
        Command::Status => "status",
        Command::Review(_) => "review",
        Command::Session(_) => "session",
        Command::Sessions(_) => "sessions",
        Command::Comments(_) => "comments",
        Command::Serve { .. } => "serve",
    };
    tokio::runtime::Runtime::new()?.block_on(
        async {
            match cli.command {
                Command::Up(options) if options.foreground => {
                    server::serve(options.port, &store, false).await
                }
                Command::Up(options) => daemon::up(options.port, !options.no_open, &store).await,
                Command::Down => daemon::down(&store).await,
                Command::Status => daemon::status(&store, cli.json).await,
                Command::Review(args) => reviews::review(args, &store, cli.json).await,
                Command::Session(args) => reviews::session(args, &store, cli.json).await,
                Command::Sessions(args) => reviews::sessions(args, &store, cli.json).await,
                Command::Comments(args) => reviews::comments(args, &store, cli.json).await,
                Command::Serve { port, daemonize } => server::serve(port, &store, daemonize).await,
            }
        }
        .instrument(tracing::info_span!("cli_command", command)),
    )
}
