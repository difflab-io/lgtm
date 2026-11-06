//! Injectable application services.
mod auth;
mod review;

pub use auth::AuthService;
pub use review::ReviewService;

mod snapshot;
pub use snapshot::SnapshotService;

mod actor;

pub mod workspace;

pub mod forge;

pub mod diff;
