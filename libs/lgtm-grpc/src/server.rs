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
use serde::{Serialize, de::DeserializeOwned};
use tower_http::cors::{Any, CorsLayer};

use crate::error::{ConnectErrorBody, ServiceError};
use crate::proto::*;
use crate::service::LgtmService;

type ServiceState = Arc<dyn LgtmService>;

pub fn build_router(service: ServiceState) -> Router {
    Router::new()
        .route(
            "/lgtm.v1.LgtmService/GetChangedFiles",
            post(get_changed_files),
        )
        .route(
            "/lgtm.v1.LgtmService/GetFileDiff",
            post(get_file_diff),
        )
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
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
                StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
    }
}

// Intentionally unused: silences dead-code if a caller only uses one half.
#[allow(dead_code)]
fn _assert_bounds<T: DeserializeOwned + Serialize>() {}
