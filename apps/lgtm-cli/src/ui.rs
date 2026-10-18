use crate::{daemon, store::Store};
use anyhow::{Context, Result};
use std::path::Path;

pub trait ReviewUrlBuilder {
    fn build(
        &self,
        base: &str,
        path: &Path,
        repository: Option<&str>,
        session: Option<&str>,
    ) -> Result<reqwest::Url>;
}
pub struct EncodedReviewUrl;
impl ReviewUrlBuilder for EncodedReviewUrl {
    fn build(
        &self,
        base: &str,
        path: &Path,
        repository: Option<&str>,
        session: Option<&str>,
    ) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(base)?;
        url.query_pairs_mut().append_pair(
            "path",
            path.to_str()
                .context("The review path is not valid UTF-8")?,
        );
        if let Some(repo) = repository {
            url.query_pairs_mut().append_pair("repo", repo);
        }
        if let Some(session) = session {
            url.query_pairs_mut().append_pair("session", session);
        }
        Ok(url)
    }
}
pub trait BrowserLauncher {
    fn open(&self, url: &str) -> Result<()>;
}
pub struct SystemBrowser;
impl BrowserLauncher for SystemBrowser {
    fn open(&self, url: &str) -> Result<()> {
        Ok(webbrowser::open(url)?)
    }
}
pub async fn open_review(
    path: &Path,
    repository: Option<&str>,
    session: Option<&str>,
    port: u16,
    open_browser: bool,
    store: &Store,
) -> Result<()> {
    let instance = daemon::ensure_running(port, store).await?;
    let url = EncodedReviewUrl.build(&instance.url(), path, repository, session)?;
    println!("{url}");
    if open_browser {
        launch(&SystemBrowser, url.as_str());
    }
    Ok(())
}
fn launch(browser: &impl BrowserLauncher, url: &str) {
    if let Err(error) = browser.open(url) {
        eprintln!("Could not open a browser ({error}); open the URL above.");
    }
}
pub(crate) fn open(url: &str) {
    launch(&SystemBrowser, url);
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_round_trips_context_and_fake_browser_receives_it() {
        let url = EncodedReviewUrl
            .build(
                "http://127.0.0.1:1234",
                Path::new("/plans/space & plus+ café #file"),
                Some("github.com/acme/repo"),
                Some("session-id"),
            )
            .unwrap();
        let values: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(values["path"], "/plans/space & plus+ café #file");
        assert_eq!(values["repo"], "github.com/acme/repo");
        assert_eq!(values["session"], "session-id");
        struct Fake(std::cell::RefCell<Option<String>>);
        impl BrowserLauncher for Fake {
            fn open(&self, url: &str) -> Result<()> {
                *self.0.borrow_mut() = Some(url.into());
                Ok(())
            }
        }
        let fake = Fake(std::cell::RefCell::new(None));
        launch(&fake, url.as_str());
        assert_eq!(fake.0.borrow().as_deref(), Some(url.as_str()));
    }
}
