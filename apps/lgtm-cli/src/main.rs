use clap::{Args, Parser, Subcommand};

#[path = "../bundle/backend/lib.rs"]
pub mod backend;
mod context;
mod daemon;
#[path = "../bundle/db/src/lib.rs"]
pub mod database;
mod log;

mod reviews;
mod server;
mod store;
mod ui;

// types -----------------------------------------------------------------------

/// Run LGTM with a bundled web UI and gRPC server.
#[derive(Parser)]
#[command(name = "lgtm", version, about)]
struct Cli {
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
    Status {
        /// Print server discovery information as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Open the current directory in the web interface.
    Review(reviews::ReviewArgs),
    /// Start, inspect, complete, export, refresh or submit a review session.
    Session(reviews::SessionArgs),
    /// List sessions using inferred path/repository filters.
    Sessions(reviews::SessionsArgs),
    /// Add, reply to, list or resolve context-owned comments.
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

fn main() -> anyhow::Result<()> {
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
    tokio::runtime::Runtime::new()?.block_on(async {
        match cli.command {
            Command::Up(options) if options.foreground => {
                server::serve(options.port, &store, false).await
            }
            Command::Up(options) => daemon::up(options.port, !options.no_open, &store).await,
            Command::Down => daemon::down(&store).await,
            Command::Status { json } => daemon::status(&store, json).await,
            Command::Review(args) => reviews::review(args, &store).await,
            Command::Session(args) => reviews::session(args, &store).await,
            Command::Sessions(args) => reviews::sessions(args, &store).await,
            Command::Comments(args) => reviews::comments(args, &store).await,
            Command::Serve { port, daemonize } => server::serve(port, &store, daemonize).await,
        }
    })
}
