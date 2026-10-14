use crate::{daemon, store::Store};
use anyhow::{Context, Result};
use std::path::Path;

// api -------------------------------------------------------------------------

pub async fn open_review_ui_at_path(
    path: &Path,
    port: u16,
    open_browser: bool,
    store: &Store,
) -> Result<()> {
    let path = path
        .to_str()
        .context("The review path is not valid UTF-8")?;
    let instance = daemon::ensure_running(port, store).await?;
    let mut url = reqwest::Url::parse(&instance.url())?;
    url.query_pairs_mut().append_pair("path", path);
    println!("{url}");
    if open_browser {
        open(url.as_str());
    }
    Ok(())
}

// helpers ---------------------------------------------------------------------

pub(crate) fn open(url: &str) {
    if let Err(error) = webbrowser::open(url) {
        eprintln!("Could not open a browser ({error}); open the URL above.");
    }
}
