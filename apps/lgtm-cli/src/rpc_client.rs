//! Generated review client; command modules do not own transport details.
use crate::{
    backend::proto::lgtm::v1::{HelloReviewRequest, HelloReviewResponse, ReviewServiceClient},
    store::Store,
};
use anyhow::{Context, Result};
use connectrpc::{
    Protocol,
    client::{CallOptions, ClientConfig, HttpClient},
};
use std::time::Duration;

pub struct Client {
    endpoint_url: String,
}

impl Client {
    pub fn new(store: &Store) -> Result<Self> {
        let endpoint_url = std::env::var("LGTM_RPC_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                store
                    .instance()
                    .map(|instance| format!("{}/rpc", instance.url()))
            })
            .context("LGTM backend is unavailable; run lgtm up --no-open or set LGTM_RPC_URL")?;
        Ok(Self { endpoint_url })
    }

    // Public ------------------------------------------------------------------

    pub async fn get_hello(&self, name: &str) -> Result<HelloReviewResponse> {
        let config = ClientConfig::new(self.endpoint_url.parse()?).with_protocol(Protocol::Connect);
        let response = ReviewServiceClient::new(HttpClient::plaintext(), config)
            .hello_review_with_options(
                HelloReviewRequest {
                    name: name.into(),
                    ..Default::default()
                },
                CallOptions::default().with_timeout(Duration::from_secs(5)),
            )
            .await?;
        Ok(response.into_owned())
    }
}
