//! Reusable LGTM backend with a standard Connect/gRPC health service.

use connectrpc::Router;
use connectrpc_health::{StaticChecker, install_static};
use std::sync::Arc;

/// Construct the RPC router and its whole-process health status controller.
pub fn router() -> (Router, Arc<StaticChecker>) {
    install_static(Router::new(), std::iter::empty::<&str>())
}
