use crate::{backend, store::Store};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use std::{future::IntoFuture, time::Duration};
use tower::ServiceExt;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

// api -------------------------------------------------------------------------

pub async fn serve(port: u16, store: &Store, detached: bool) -> anyhow::Result<()> {
    store.ensure_exists()?;
    // daemonize owns the same PID-file lock in background mode.
    let _lock = if detached {
        None
    } else {
        Some(store.foreground_pid_lock()?)
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let port = listener.local_addr()?.port();
    let database_url = std::env::var("LGTM_DATABASE_URL").unwrap_or_else(|_| {
        format!(
            "sqlite://{}?mode=rwc",
            store.path("reviews.sqlite").display()
        )
    });
    let database = backend::database::Database::connect(&database_url).await?;
    let (router, health) = backend::router_with_database(database);
    let rpc = router.into_axum_service();
    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"pid": std::process::id()})) }),
        )
        .nest_service("/rpc", rpc.clone())
        .fallback(dispatch)
        .with_state(rpc);
    store.write_port(port)?;
    tracing::info!(port, pid = std::process::id(), "Server started");
    if !detached {
        println!("LGTM UI and RPC server listening on http://127.0.0.1:{port}");
    }
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let serving = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            signal().await;
            health.shutdown();
            let _ = shutdown_tx.send(());
        })
        .into_future();
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        _ = shutdown_rx => tokio::time::timeout(Duration::from_secs(5), &mut serving).await.unwrap_or(Ok(())),
    };
    store.clear_port();
    tracing::info!("Server stopped");
    result?;
    Ok(())
}

// helpers ---------------------------------------------------------------------

async fn dispatch(
    State(rpc): State<connectrpc::ConnectRpcService>,
    request: Request<Body>,
) -> Response {
    if request.method() == Method::GET || request.method() == Method::HEAD {
        let path = request.uri().path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        return match assets::get(path) {
            Some((bytes, mime)) => (
                [("content-type", mime), ("cache-control", "no-cache")],
                if request.method() == Method::HEAD {
                    &[][..]
                } else {
                    bytes
                },
            )
                .into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    match rpc.oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(error) => match error {},
    }
}

async fn signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install termination handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.ok();
}
