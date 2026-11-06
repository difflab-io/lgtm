//! Small authenticated snapshot service. Development tokens simulate signed-in accounts.
mod entities;
mod repository;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use lgtm_db::sync::{Account, Pull, Push};
use repository::Repository;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
struct App {
    repository: Repository,
    users: Arc<HashMap<String, Uuid>>,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let url = std::env::var("LGTM_SYNC_DATABASE_URL")?;
    let users: HashMap<String, Uuid> = serde_json::from_str(&std::env::var("LGTM_SYNC_USERS")?)?;
    anyhow::ensure!(
        !users.is_empty() && users.keys().all(|key| !key.is_empty()),
        "Configure nonempty development tokens"
    );
    let app = App {
        repository: Repository::connect(&url).await?,
        users: Arc::new(users),
    };
    let router = Router::new()
        .route("/v1/account", get(account))
        .route("/v1/push", post(push))
        .route("/v1/pull", get(pull))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(app);
    let address = std::env::var("LGTM_SYNC_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    eprintln!("LGTM sync service listening on {}", listener.local_addr()?);
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn account(State(app): State<App>, headers: HeaderMap) -> ApiResult<Account> {
    Ok(Json(Account {
        id: authenticate(&app, &headers)?,
    }))
}

#[tracing::instrument(skip_all)]
async fn push(
    State(app): State<App>,
    headers: HeaderMap,
    Json(push): Json<Push>,
) -> ApiResult<lgtm_db::sync::Change> {
    let owner = authenticate(&app, &headers)?;
    push.snapshot
        .validate()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    match app.repository.push(owner, push).await.map_err(internal)? {
        Some(change) => Ok(Json(change)),
        None => Err((
            StatusCode::CONFLICT,
            "Review changed remotely; local changes must be reconciled".into(),
        )),
    }
}

#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    cursor: i64,
}

#[tracing::instrument(skip_all)]
async fn pull(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<Cursor>,
) -> ApiResult<Pull> {
    let owner = authenticate(&app, &headers)?;
    if query.cursor < 0 {
        return Err((StatusCode::BAD_REQUEST, "Cursor cannot be negative".into()));
    }
    Ok(Json(
        app.repository
            .get_changes(owner, query.cursor)
            .await
            .map_err(internal)?,
    ))
}

fn authenticate(app: &App, headers: &HeaderMap) -> Result<Uuid, (StatusCode, String)> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|token| app.users.get(token))
        .copied()
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "Sign in with a configured development token".into(),
        ))
}

fn internal(error: impl std::fmt::Display) -> (StatusCode, String) {
    eprintln!("Sync error: {error}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Sync storage failed".into(),
    )
}
