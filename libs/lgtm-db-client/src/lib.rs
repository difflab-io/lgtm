//! lgtm-db-client: SQLite-backed storage for the lgtm review PoC.
//!
//! Scope: identity (users), review threads, and comments. The schema lives in
//! `schema::INIT_SQL`; each row-mapping helper is a private function in `db`.
//! The crate intentionally does not depend on lgtm-grpc, so the proto layer
//! translates between its wire types and these row types.

mod db;
mod schema;

pub use db::{CommentRow, Db, ThreadRow, UserRow};
