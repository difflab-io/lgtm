//! Idempotent migration SQL. The PoC uses a single `reviews` row-per-launch
//! so operators can see the server has booted and connected to the db.

pub const INIT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_path TEXT NOT NULL,
    base_ref  TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
"#;
