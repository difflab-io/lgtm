//! Standalone server binary: starts the Connect-JSON HTTP server and opens
//! the shared SQLite db at `.tmp/db.sqlite` relative to CWD.

use std::sync::Arc;

use anyhow::Context;
use lgtm_db_client::Db;
use lgtm_grpc::{server::build_router, service::DefaultService};
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDR: &str = "127.0.0.1:50051";
const DB_PATH: &str = ".tmp/db.sqlite";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let db = Arc::new(
        Db::open_at(DB_PATH).with_context(|| format!("open sqlite db at {DB_PATH}"))?,
    );
    tracing::info!(path = DB_PATH, "sqlite db opened");

    let addr: std::net::SocketAddr = std::env::var("LGTM_ADDR")
        .unwrap_or_else(|_| DEFAULT_ADDR.into())
        .parse()
        .context("parse LGTM_ADDR")?;

    let service = Arc::new(DefaultService::new(db));
    let app = build_router(service);

    let listener = tokio::net::TcpListener::bind(addr).await.context("bind listener")?;
    tracing::info!(%addr, "lgtm-grpc listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
