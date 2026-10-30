//! Browser launching shared by daemon and UI commands.
use anyhow::Result;

pub trait BrowserLauncher {
    fn open(&self, url: &str) -> Result<()>;
}

pub struct SystemBrowser;

impl BrowserLauncher for SystemBrowser {
    fn open(&self, url: &str) -> Result<()> {
        Ok(webbrowser::open(url)?)
    }
}

pub(crate) fn open(url: &str) {
    if let Err(error) = SystemBrowser.open(url) {
        eprintln!("Could not open a browser ({error}); open {url} manually.");
    }
}
