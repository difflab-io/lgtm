//! Idempotent migration SQL. Order matters: users ← threads ← comments.
//!
//! All timestamps are unix seconds. Soft-deletes aren't modelled; nothing
//! in the PoC deletes rows.

pub const INIT_SQL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('user','agent','unspecified')),
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    UNIQUE(name, kind)
);

CREATE TABLE IF NOT EXISTS threads (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_path  TEXT    NOT NULL,
    base_ref   TEXT    NOT NULL,
    file_path  TEXT    NOT NULL,
    line       INTEGER NOT NULL DEFAULT 0,
    resolved   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS threads_by_repo_base
    ON threads(repo_path, base_ref);
CREATE INDEX IF NOT EXISTS threads_by_file
    ON threads(repo_path, base_ref, file_path);

CREATE TABLE IF NOT EXISTS comments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id  INTEGER NOT NULL REFERENCES threads(id),
    author_id  INTEGER NOT NULL REFERENCES users(id),
    body       TEXT    NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS comments_by_thread
    ON comments(thread_id, created_at);

-- Legacy table from the first PoC pass; keeping the migration so existing
-- .tmp/db.sqlite instances don't complain. Nothing writes to it anymore.
CREATE TABLE IF NOT EXISTS reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_path TEXT NOT NULL,
    base_ref  TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
"#;
