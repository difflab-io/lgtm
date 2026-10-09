//! lgtm-db-client: SQLite-backed storage for the lgtm review PoC.
//!
//! Scope is deliberately tiny — a single `reviews` table — because the PoC
//! focuses on the UI stack comparison, not persistence. Expand the schema as
//! the UI starts needing it.

mod db;
mod schema;

pub use db::Db;
