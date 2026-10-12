use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[path = "../bundle/backend.rs"]
pub mod backend;
mod daemon;
mod server;

/// Run LGTM with a bundled web UI and gRPC server.
#[derive(Parser)]
#[command(name = "lgtm", version, about)]
struct Cli {
    /// Directory for daemon state and logs.
    #[arg(long, global = true, env = "LGTM_STATE_DIR")]
    state_dir: Option<PathBuf>,
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
    #[command(hide = true)]
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let state_dir = daemon::state_dir(cli.state_dir)?;
    match cli.command {
        Command::Up(options) if options.foreground => server::serve(options.port, &state_dir).await,
        Command::Up(options) => daemon::up(options.port, !options.no_open, &state_dir).await,
        Command::Down => daemon::down(&state_dir).await,
        Command::Status => daemon::status(&state_dir).await,
        Command::Serve { port } => server::serve(port, &state_dir).await,
    }
}
