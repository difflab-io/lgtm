//! Generated review client factory; command modules call `ReviewServiceClient` methods directly.

use std::time::Duration;

use anyhow::Result;
use connectrpc::{
    Protocol,
    client::{ClientConfig, HttpClient},
};

use crate::{backend::proto::lgtm::v1::ReviewServiceClient, daemon, store::Store};

const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Connect to `LGTM_RPC_URL` if set, or reuse/auto-start the background daemon from `store`, returning the generated `ReviewServiceClient`.
pub async fn connect(store: &Store) -> Result<ReviewServiceClient<HttpClient>> {
    let endpoint_url = if let Ok(url) = std::env::var("LGTM_RPC_URL")
        && !url.trim().is_empty()
    {
        url.trim().to_owned()
    } else {
        let instance = daemon::ensure_running(0, store).await?;
        format!("{}/rpc", instance.url())
    };
    let config = ClientConfig::new(endpoint_url.parse()?)
        .with_protocol(Protocol::Connect)
        .with_default_timeout(RPC_TIMEOUT);
    Ok(ReviewServiceClient::new(HttpClient::plaintext(), config))
}
