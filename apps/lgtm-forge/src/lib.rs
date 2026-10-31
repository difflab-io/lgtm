//! Reusable LGTM backend library and Connect RPC server.

pub use lgtm_db as database;
pub mod error;
pub mod extensions;
pub mod forge;
pub mod models;
pub mod rpc;
pub mod services;
pub mod snapshots;
pub mod utils;

use std::sync::Arc;

use connectrpc::{Encodable, RequestContext, Response, Router, ServiceRequest, ServiceResult};
use connectrpc_health::{StaticChecker, install_static};

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

/// Construct the RPC router with default database, snapshot store, and GitHub forge adapters.
pub fn router() -> (Router, Arc<StaticChecker>) {
    router_with_database(database::Database::default())
}

/// Connect to the default persistent database (`$LGTM_HOME/lgtm.db` or `LGTM_DATABASE_URL`) and construct the RPC router.
pub async fn connect_default_router() -> error::Result<(Router, Arc<StaticChecker>)> {
    let database_url = utils::default_database_url()?;
    let db = database::Database::connect(&database_url).await?;
    Ok(router_with_database(db))
}

/// Construct a router sharing one database across application services.
pub fn router_with_database(database: database::Database) -> (Router, Arc<StaticChecker>) {
    router_with_dependencies(
        database,
        snapshots::SnapshotStore::default(),
        Arc::new(forge::GitHubForge::new()),
    )
}

/// Construct a router with explicit database, snapshot store, and remote forge dependencies.
pub fn router_with_dependencies(
    database: database::Database,
    snapshots: snapshots::SnapshotStore,
    remote_forge: Arc<dyn forge::RemoteForge>,
) -> (Router, Arc<StaticChecker>) {
    install_static(
        Router::new()
            .add_service(Arc::new(Greeter))
            .add_service(Arc::new(rpc::ReviewApi::with_dependencies(
                database,
                snapshots,
                remote_forge,
            ))),
        [
            GREETER_SERVICE_SERVICE_NAME,
            proto::lgtm::v1::REVIEW_SERVICE_SERVICE_NAME,
        ],
    )
}
