use crate::{
    backend,
    daemon::{self, Instance},
};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{future::IntoFuture, path::Path, sync::Arc, time::Duration};
use tokio::sync::Notify;
use tower::ServiceExt;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

#[derive(Clone)]
struct AppState {
    rpc: connectrpc::ConnectRpcService,
    instance: Instance,
    shutdown: Arc<Notify>,
}

fn authorized(headers: &HeaderMap, instance: &Instance) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some(format!("Bearer {}", instance.token).as_str())
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.instance) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(serde_json::json!({"pid": state.instance.pid, "version": env!("CARGO_PKG_VERSION")}))
        .into_response()
}

async fn shutdown(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.instance) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.shutdown.notify_one();
    StatusCode::ACCEPTED.into_response()
}

async fn dispatch(State(state): State<AppState>, request: Request<Body>) -> Response {
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
    match state.rpc.oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(error) => match error {},
    }
}

async fn signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install termination handler");
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    ctrl_c.await;
}

pub async fn serve(port: u16, dir: &Path) -> anyhow::Result<()> {
    daemon::prepare(dir)?;
    let _lock = daemon::lock(dir, "server.lock")?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let instance = Instance {
        pid: std::process::id(),
        port: listener.local_addr()?.port(),
        token: uuid::Uuid::new_v4().to_string(),
    };
    let (router, health) = backend::router();
    let rpc = router.into_axum_service();
    let notify = Arc::new(Notify::new());
    let state = AppState {
        rpc: rpc.clone(),
        instance: instance.clone(),
        shutdown: notify.clone(),
    };
    let app = Router::new()
        .route("/_lgtm/status", get(status))
        .route("/_lgtm/shutdown", post(shutdown))
        .nest_service("/rpc", rpc)
        .fallback(dispatch)
        .with_state(state);
    daemon::write(dir, &instance)?;
    println!("LGTM UI and RPC server listening on {}", instance.url());
    let draining = Arc::new(Notify::new());
    let shutdown_started = draining.clone();
    let serving = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! { _ = signal() => {}, _ = notify.notified() => {} }
            health.shutdown();
            shutdown_started.notify_one();
        })
        .into_future();
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        _ = draining.notified() => {
            // Health-watch streams must not keep the managed process alive forever.
            match tokio::time::timeout(Duration::from_secs(5), &mut serving).await {
                Ok(result) => result,
                Err(_) => Ok(()),
            }
        }
    };
    std::fs::remove_file(dir.join("instance.json")).ok();
    result?;
    Ok(())
}
