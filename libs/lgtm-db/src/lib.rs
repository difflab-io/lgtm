//! Persist review conversations, identities and sessions through scoped repositories.
//!
//! Use [`Database`] for a shared connection pool. Begin a [`Transaction`] to compose
//! writes across repositories; commit explicitly or drop it to roll back.
//! Returned [`models`] are SeaORM values. Writes go through [`repositories`];
//! application validation and workflow orchestration belong to backend services.
//!
//! # Examples
//!
//! ```
//! use lgtm_db::{Database, models::identities, types::IdentityKind};
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let db = Database::connect("sqlite::memory:").await?;
//! let actor = identities::Model {
//!     id: uuid::Uuid::new_v4(), name: "Alice".into(), kind: IdentityKind::Human,
//!     email: Some("alice@example.com".into()), model: None, provider: None,
//!     forge_account: String::new(),
//! };
//! let tx = db.begin().await?;
//! tx.identities.insert(&actor).await?;
//! assert_eq!(tx.identities.get_by_id(&actor.id).await?, Some(actor));
//! tx.commit().await?;
//! db.close().await;
//! # Ok(()) }
//! ```
//!
//! # Storage and features
//!
//! Internal keys are typed UUIDs. External forge IDs are opaque strings, scoped by
//! context and object kind. Repository keys are canonical natural keys, not URLs.
//! The default `sqlite` feature enables SQLite; `postgres` adds PostgreSQL.
//! One private Rust migration history is applied when opening the database.
//!
//! # API boundaries
//!
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
mod tests;

/// Typed database discriminators.
pub mod types;
