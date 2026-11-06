use connectrpc::Server;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let address: SocketAddr = std::env::var("LGTM_BACKEND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse()?;
    let database_url = std::env::var("LGTM_DATABASE_URL").unwrap_or_else(|_| {
        let home = std::env::var("LGTM_HOME").unwrap_or_else(|_| {
            format!(
                "{}/.lgtm",
                std::env::var("HOME").unwrap_or_else(|_| ".".into())
            )
        });
        let _ = std::fs::create_dir_all(&home);
        format!("pglite://{home}/reviews.pg")
    });
    let database = lgtm_backend::database::Database::connect(&database_url).await?;
    let sync = match (
        std::env::var("LGTM_SYNC_URL"),
        std::env::var("LGTM_SYNC_TOKEN"),
    ) {
        (Ok(url), Ok(token)) => Some(lgtm_sync::SyncClient::new(url, token)?),
        (Err(_), Err(_)) => None,
        _ => return Err("Set both LGTM_SYNC_URL and LGTM_SYNC_TOKEN, or neither".into()),
    };
    let sync_db = database.clone();
    let worker = sync.map(|client| {
        tokio::spawn(async move {
            loop {
                if let Err(error) = client.synchronize(&sync_db).await {
                    tracing::warn!(%error, "Sync paused; local reviews remain available");
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
    });
    let shutdown_db = database.clone();
    let (router, health) = lgtm_backend::router_with_database(database);
    let server = Server::bind(address).await?;
    eprintln!("LGTM backend listening on http://{}", server.local_addr()?);
    server
        .serve_with_graceful_shutdown(router, async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("Could not listen for Ctrl-C: {error}");
            }
            health.shutdown();
        })
        .await?;
    if let Some(worker) = worker {
        worker.abort();
        let _ = worker.await;
    }
    shutdown_db.close().await;
    Ok(())
}
