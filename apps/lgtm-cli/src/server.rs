use std::{future::IntoFuture, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use tower::ServiceExt;

use crate::store::Store;

// api -------------------------------------------------------------------------

pub async fn serve(port: u16, store: &Store, detached: bool) -> anyhow::Result<()> {
    store.ensure_exists()?;
    // daemonize owns the same PID-file lock in background mode on Unix.
    let _lock = if detached && cfg!(unix) {
        None
    } else {
        Some(store.foreground_pid_lock()?)
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::AddrInUse {
                anyhow::Error::new(err).context("Address already in use")
            } else {
                anyhow::Error::new(err)
            }
        })?;
    let port = listener.local_addr()?.port();
    let (router, health) = lgtm_forge::connect_default_router().await?;
    let rpc = router.into_axum_service();
    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"pid": std::process::id()})) }),
        )
        .nest_service("/rpc", rpc.clone())
        .fallback(dispatch)
        .with_state(rpc);
    #[cfg(not(unix))]
    std::fs::remove_file(store.path("server.stop")).ok();
    store.write_port(port)?;
    tracing::info!(port, pid = std::process::id(), "Server started");
    if !detached {
        println!("LGTM RPC server listening on http://127.0.0.1:{port}");
    }
    let stop_path = store.path("server.stop");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let serving = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            signal(&stop_path).await;
            health.shutdown();
            let _ = shutdown_tx.send(());
        })
        .into_future();
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        _ = shutdown_rx => tokio::time::timeout(Duration::from_secs(5), &mut serving).await.unwrap_or(Ok(())),
    };
    tracing::info!("Server stopped");
    drop(_lock);
    store.clear_port();
    result?;
    Ok(())
}

// helpers ---------------------------------------------------------------------

async fn dispatch(
    State(rpc): State<connectrpc::ConnectRpcService>,
    request: Request<Body>,
) -> Response {
    if request.method() == Method::GET || request.method() == Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }
    match rpc.oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(error) => match error {},
    }
}

#[cfg(unix)]
async fn signal(_stop_path: &std::path::Path) {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install termination handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
}

#[cfg(not(unix))]
async fn signal(stop_path: &std::path::Path) {
    let pid = std::process::id().to_string();
    let wait_for_stop_file = async {
        loop {
            if std::fs::read_to_string(stop_path)
                .ok()
                .is_some_and(|content| content.trim() == pid)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = wait_for_stop_file => {},
    }
}
