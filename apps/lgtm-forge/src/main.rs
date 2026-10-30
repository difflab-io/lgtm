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
    let (router, health) = lgtm_forge::router();
    let server = Server::bind(address).await?;
    eprintln!("LGTM backend listening on http://{}", server.local_addr()?);
    server
        .serve_with_graceful_shutdown(router, async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("Could not listen for Ctrl-C: {error}");
            }
            health.shutdown();
        })
        .await
}
