//! Reusable LGTM backend with greeting and health RPC services.

pub use lgtm_db as database;
pub mod error;
pub mod forge;
pub mod models;
pub mod rpc;
pub mod services;
pub mod utils;

use connectrpc::{Encodable, RequestContext, Response, Router, ServiceRequest, ServiceResult};
use connectrpc_health::{StaticChecker, install_static};
use std::sync::Arc;

/// Message types, service traits, and clients generated from the shared schema.
pub mod proto {
    connectrpc::include_generated!();
}

use proto::lgtm::v1::{
    GREETER_SERVICE_SERVICE_NAME, GreeterService, SayHelloRequest, SayHelloResponse,
};

struct Greeter;

impl GreeterService for Greeter {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.GreeterService/SayHello"))]
    async fn say_hello<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, SayHelloRequest>,
    ) -> ServiceResult<impl Encodable<SayHelloResponse> + Send + use<'a>> {
        let name = request.name.trim();
        let name = if name.is_empty() { "World" } else { name };
        Response::ok(SayHelloResponse {
            message: format!("Hello, {name}!"),
            ..Default::default()
        })
    }
}

/// Construct the RPC router and its health status controller.
pub fn router() -> (Router, Arc<StaticChecker>) {
    install_static(
        Router::new().add_service(Arc::new(Greeter)),
        [GREETER_SERVICE_SERVICE_NAME],
    )
}

/// Construct a stateful API router sharing one database across all RPC services.
pub fn router_with_database(database: database::Database) -> (Router, Arc<StaticChecker>) {
    let api = Arc::new(rpc::Api::new(database));
    install_static(
        Router::new()
            .add_service(Arc::new(Greeter))
            .add_service::<rpc::Api, proto::lgtm::v1::IdentityServiceRegisterMarker>(api.clone())
            .add_service::<rpc::Api, proto::lgtm::v1::ReviewServiceRegisterMarker>(api.clone())
            .add_service::<rpc::Api, proto::lgtm::v1::DiffServiceRegisterMarker>(api),
        [
            GREETER_SERVICE_SERVICE_NAME,
            proto::lgtm::v1::IDENTITY_SERVICE_SERVICE_NAME,
            proto::lgtm::v1::REVIEW_SERVICE_SERVICE_NAME,
            proto::lgtm::v1::DIFF_SERVICE_SERVICE_NAME,
        ],
    )
}
