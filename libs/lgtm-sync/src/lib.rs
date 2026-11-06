//! Direct push/pull client. Local review commits never depend on network availability.
use anyhow::{Context, Result, bail};
use lgtm_db::{
    Database,
    sync::{Account, Change, Pull},
};
use reqwest::Client;
use std::time::Duration;

pub struct SyncClient {
    client: Client,
    remote_url: String,
    token: String,
}

impl SyncClient {
    pub fn new(remote_url: String, token: String) -> Result<Self> {
        let url = reqwest::Url::parse(&remote_url)?;
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            bail!("Sync requires HTTPS except on loopback");
        }
        if token.is_empty() {
            bail!("Sync token cannot be empty");
        }
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            remote_url: remote_url.trim_end_matches('/').to_owned(),
            token,
        })
    }

    // Public ------------------------------------------------------------------

    /// A single worker owns this cycle. Every DB transaction finishes before HTTP.
    pub async fn synchronize(&self, db: &Database) -> Result<()> {
        let account: Account = self
            .client
            .get(format!("{}/v1/account", self.remote_url))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        db.sync.bind_account(account.id).await?;
        for id in db.sync.get_context_ids().await? {
            let tx = db.begin().await?;
            let snapshot = tx.sync.get_snapshot(id).await?;
            let pending = tx.sync.enqueue(snapshot).await?;
            tx.commit().await?;
            if let Some(push) = pending {
                let response = self
                    .client
                    .post(format!("{}/v1/push", self.remote_url))
                    .bearer_auth(&self.token)
                    .json(&push)
                    .send()
                    .await?;
                if response.status() == reqwest::StatusCode::CONFLICT {
                    bail!("Sync conflict for {id}; local review and pending upload retained");
                }
                let change: Change = response.error_for_status()?.json().await?;
                // Acknowledging the sent baseline leaves any newer local edits dirty.
                db.sync.acknowledge(&push, change.version).await?;
            }
        }
        let cursor = db.sync.get_cursor().await?;
        let pull: Pull = self
            .client
            .get(format!("{}/v1/pull", self.remote_url))
            .query(&[("cursor", cursor)])
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let tx = db.begin().await?;
        for change in pull.changes {
            tx.sync
                .accept(&change.snapshot, change.version)
                .await
                .context("Applying downloaded review")?;
        }
        tx.sync.advance_cursor(pull.cursor).await?;
        tx.commit().await?;
        Ok(())
    }
}
