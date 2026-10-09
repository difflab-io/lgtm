//! Thin axum layer that exposes [`LgtmService`] over the Connect-JSON unary
//! protocol. Each RPC is served at `POST /{package}.{Service}/{Method}` with
//! an `application/json` body.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};

use crate::error::{ConnectErrorBody, ServiceError};
use crate::proto::*;
use crate::service::LgtmService;

type ServiceState = Arc<dyn LgtmService>;

pub fn build_router(service: ServiceState) -> Router {
    Router::new()
        // Diff
        .route("/lgtm.v1.LgtmService/GetChangedFiles", post(get_changed_files))
        .route("/lgtm.v1.LgtmService/GetFileDiff", post(get_file_diff))
        // Identity
        .route("/lgtm.v1.LgtmService/UpsertUser", post(upsert_user))
        // Comments
        .route("/lgtm.v1.LgtmService/ListThreads", post(list_threads))
        .route("/lgtm.v1.LgtmService/CreateThread", post(create_thread))
        .route("/lgtm.v1.LgtmService/AppendComment", post(append_comment))
        .route("/lgtm.v1.LgtmService/ResolveThread", post(resolve_thread))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(service)
}

async fn get_changed_files(
    State(svc): State<ServiceState>,
    Json(req): Json<GetChangedFilesRequest>,
) -> Response {
    reply(svc.get_changed_files(req))
}

async fn get_file_diff(
    State(svc): State<ServiceState>,
    Json(req): Json<GetFileDiffRequest>,
) -> Response {
    reply(svc.get_file_diff(req))
}

async fn upsert_user(
    State(svc): State<ServiceState>,
    Json(req): Json<UpsertUserRequest>,
) -> Response {
    reply(svc.upsert_user(req))
}

async fn list_threads(
    State(svc): State<ServiceState>,
    Json(req): Json<ListThreadsRequest>,
) -> Response {
    reply(svc.list_threads(req))
}

async fn create_thread(
    State(svc): State<ServiceState>,
    Json(req): Json<CreateThreadRequest>,
) -> Response {
    reply(svc.create_thread(req))
}

async fn append_comment(
    State(svc): State<ServiceState>,
    Json(req): Json<AppendCommentRequest>,
) -> Response {
    reply(svc.append_comment(req))
}

async fn resolve_thread(
    State(svc): State<ServiceState>,
    Json(req): Json<ResolveThreadRequest>,
) -> Response {
    reply(svc.resolve_thread(req))
}

fn reply<T: Serialize>(result: Result<T, ServiceError>) -> Response {
    match result {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(err) => {
            let code = err.code();
            let body = ConnectErrorBody {
                code: code.as_str(),
                message: err.to_string(),
            };
            (
                StatusCode::from_u16(code.http_status())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
    }
}
