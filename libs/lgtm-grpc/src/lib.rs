//! lgtm-grpc: Connect-JSON unary service powering the lgtm code-review PoC.
//!
//! Public surface:
//! - [`proto`] — serde message types mirroring `proto/lgtm.proto`
//! - [`service::LgtmService`] trait + [`service::DefaultService`]
//! - [`server::build_router`] — axum Router wiring the service to HTTP
//! - [`error::ServiceError`] — error enum returned by the service trait

pub mod error;
pub mod git;
pub mod proto;
pub mod server;
pub mod service;
