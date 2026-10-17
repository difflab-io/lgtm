//! Entity repositories, SeaORM models, migrations and transactions.
//! Entity module paths are private; row models are exported via `models`:
//! ```compile_fail
//! use lgtm_db::entities::comments::Model;
//! ```
//! Raw connections are not part of the client API:
//! ```compile_fail
//! fn inspect(db: &lgtm_db::Database) { let _ = db.connection(); }
//! ```
//! Model modules do not re-export active model or entity names:
//! ```compile_fail
//! use lgtm_db::models::comments::ActiveModel;
//! ```
//! ```compile_fail
//! use lgtm_db::models::comments::Entity;
//! ```
//! Returned models retain their SeaORM traits, but the database handle cannot
//! be used as a raw connection for direct ORM writes:
//! ```compile_fail
//! use sea_orm::{ActiveModelTrait, IntoActiveModel};
//! async fn bypass(db: &lgtm_db::Database, comment: lgtm_db::models::comments::Model) {
//!     comment.into_active_model().update(db).await.unwrap();
//! }
//! ```
//! Migration internals are not a client operation:
//! ```compile_fail
//! use lgtm_db::migration::Migrator;
//! ```
//! Profile editing is not exposed:
//! ```compile_fail
//! async fn rename(db: &lgtm_db::Database) {
//!     db.identities.rename("id", "Name", "email@example.com").await;
//! }
//! ```
mod connection;
mod entities;
pub mod error;
mod migration;
pub use connection::{Database, Transaction};

pub mod models;
pub mod repositories;

#[cfg(test)]
mod test_support;
