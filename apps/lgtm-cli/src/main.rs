use clap::{Args, Parser, Subcommand};

#[path = "../bundle/backend.rs"]
pub mod backend;
mod daemon;
mod log;
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
    Review {
        /// Server port when starting a new instance (0 selects a free port).
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Print the review URL without opening a browser.
        #[arg(long)]
        no_open: bool,
    },
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
            Command::Review { port, no_open } => {
                ui::open_review_ui_at_path(&std::env::current_dir()?, port, !no_open, &store).await
            }
            Command::Serve { port, daemonize } => server::serve(port, &store, daemonize).await,
        }
    })
}
