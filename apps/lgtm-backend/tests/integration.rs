//! Backend service and RPC integration contracts.
mod support;

#[path = "integration/auth_service.rs"]
mod auth_service;
#[path = "integration/greeting_service.rs"]
mod greeting_service;
#[path = "integration/health_service.rs"]
mod health_service;
#[path = "integration/review_service/mod.rs"]
mod review_service;
