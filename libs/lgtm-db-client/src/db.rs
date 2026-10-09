//! Thin ergonomic wrapper around a `rusqlite::Connection`.
//!
//! `Db::open_at` ensures the parent directory exists, applies the init SQL,
//! and hands back a connection. The handle is `Send` because callers (e.g.
//! the server binary) store it in a tokio runtime.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use rusqlite::{Connection, params};

use crate::schema::INIT_SQL;

pub struct Db {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open_at(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db parent dir {}", parent.display()))?;
        }
        let conn = Connection::open(&path).with_context(|| format!("open sqlite at {}", path.display()))?;
        conn.execute_batch(INIT_SQL).context("apply schema")?;
        Ok(Self { path, conn: Mutex::new(conn) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_review(&self, repo_path: &str, base_ref: &str) -> anyhow::Result<i64> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO reviews (repo_path, base_ref) VALUES (?1, ?2)",
            params![repo_path, base_ref],
        )
        .context("insert review")?;
        Ok(conn.last_insert_rowid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir_free::TempDir;

    #[test]
    fn record_review_assigns_monotonic_ids() {
        let tmp = TempDir::new();
        let path = tmp.path().join("db.sqlite");
        let db = Db::open_at(&path).unwrap();

        let first = db.record_review("/repo", "main").unwrap();
        let second = db.record_review("/repo", "main").unwrap();
        assert!(second > first);
    }
}

/// Minimal temp-dir helper so tests don't pull in the `tempdir` crate.
#[cfg(test)]
mod tempdir_free {
    use std::path::PathBuf;

    pub struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new() -> Self {
            let base = std::env::temp_dir()
                .join(format!("lgtm-db-{}", std::process::id()))
                .join(format!("{}", rand_u64()));
            std::fs::create_dir_all(&base).unwrap();
            Self(base)
        }
        pub fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn rand_u64() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }
}
