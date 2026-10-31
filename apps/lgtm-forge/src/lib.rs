//! Reusable LGTM backend with hello-review, greeting and health RPC services.

pub use lgtm_db as database;
pub mod error;
pub mod models;
pub mod rpc;
pub mod services;

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

/// Construct the RPC router with the stateless demonstration data source.
pub fn router() -> (Router, Arc<StaticChecker>) {
    router_with_database(database::Database::default())
}

/// Construct a router sharing one data source across application services.
pub fn router_with_database(database: database::Database) -> (Router, Arc<StaticChecker>) {
    install_static(
        Router::new()
            .add_service(Arc::new(Greeter))
            .add_service(Arc::new(rpc::ReviewApi::new(database))),
        [
            GREETER_SERVICE_SERVICE_NAME,
            proto::lgtm::v1::REVIEW_SERVICE_SERVICE_NAME,
        ],
    )
}
