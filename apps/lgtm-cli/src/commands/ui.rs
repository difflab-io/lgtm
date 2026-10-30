use crate::store::Store;
use clap::Args;

#[derive(Args)]
pub struct UiArgs {
    /// Preferred port when starting a new instance (0 selects a free port).
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    /// Print the URL without opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

pub async fn run(args: UiArgs, store: &Store, json: bool) -> anyhow::Result<()> {
    let instance = crate::daemon::ensure_running(args.port, store).await?;
    let url = instance.url();
    if json {
        println!("{}", serde_json::json!({"ui_url": url}));
    } else {
        println!("{url}");
    }
    if !args.no_open {
        crate::ui::open(&url);
    }
    Ok(())
}
