//! Injectable application services.
mod auth;
mod review;
mod source_locations;

pub use auth::AuthService;
pub use review::ReviewService;

mod actor;

pub mod workspace;

pub mod forge;

pub mod diff;
