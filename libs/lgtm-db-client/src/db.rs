//! Thin ergonomic wrapper around a `rusqlite::Connection`.
//!
//! `Db::open_at` ensures the parent directory exists, applies the init SQL,
//! and hands back a connection. The connection is wrapped in a `Mutex` so
//! callers can share one `Arc<Db>` across axum handlers.
//!
//! Row types here are plain structs that mirror the lgtm-grpc `proto`
//! messages but intentionally live in the db crate so lgtm-db-client stays
//! free of any gRPC dependency. The server layer in lgtm-grpc converts.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::schema::INIT_SQL;

// -------- Row types --------

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub id: i64,
    pub repo_path: String,
    pub base_ref: String,
    pub file_path: String,
    pub line: i32,
    pub resolved: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CommentRow {
    pub id: i64,
    pub thread_id: i64,
    pub author: UserRow,
    pub body: String,
    pub created_at: i64,
}

// -------- Db handle --------

pub struct Db {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open_at(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db parent dir {}", parent.display()))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("open sqlite at {}", path.display()))?;
        conn.execute_batch(INIT_SQL).context("apply schema")?;
        Ok(Self { path, conn: Mutex::new(conn) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---- Users ----

    pub fn upsert_user(&self, name: &str, kind: &str) -> Result<UserRow> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO users (name, kind) VALUES (?1, ?2)
             ON CONFLICT(name, kind) DO NOTHING",
            params![name, kind],
        )
        .context("upsert user")?;
        let row = conn
            .query_row(
                "SELECT id, name, kind, created_at FROM users
                 WHERE name = ?1 AND kind = ?2",
                params![name, kind],
                map_user,
            )
            .context("select upserted user")?;
        Ok(row)
    }

    pub fn get_user(&self, id: i64) -> Result<UserRow> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.query_row(
            "SELECT id, name, kind, created_at FROM users WHERE id = ?1",
            params![id],
            map_user,
        )
        .with_context(|| format!("user {id} not found"))
    }

    // ---- Threads + comments ----

    pub fn create_thread(
        &self,
        repo_path: &str,
        base_ref: &str,
        file_path: &str,
        line: i32,
        author_id: i64,
        body: &str,
    ) -> Result<(ThreadRow, CommentRow)> {
        let mut conn = self.conn.lock().expect("db mutex poisoned");
        let tx = conn.transaction().context("begin tx")?;
        tx.execute(
            "INSERT INTO threads (repo_path, base_ref, file_path, line, resolved)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![repo_path, base_ref, file_path, line],
        )
        .context("insert thread")?;
        let thread_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO comments (thread_id, author_id, body)
             VALUES (?1, ?2, ?3)",
            params![thread_id, author_id, body],
        )
        .context("insert first comment")?;
        let comment_id = tx.last_insert_rowid();

        let thread = tx
            .query_row(
                "SELECT id, repo_path, base_ref, file_path, line, resolved, created_at
                 FROM threads WHERE id = ?1",
                params![thread_id],
                map_thread,
            )
            .context("select created thread")?;
        let comment = select_comment(&tx, comment_id)?;
        tx.commit().context("commit tx")?;
        Ok((thread, comment))
    }

    pub fn append_comment(
        &self,
        thread_id: i64,
        author_id: i64,
        body: &str,
    ) -> Result<CommentRow> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM threads WHERE id = ?1",
                params![thread_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .context("probe thread")?;
        if exists.is_none() {
            bail!("thread {thread_id} not found");
        }
        conn.execute(
            "INSERT INTO comments (thread_id, author_id, body) VALUES (?1, ?2, ?3)",
            params![thread_id, author_id, body],
        )
        .context("insert comment")?;
        let comment_id = conn.last_insert_rowid();
        select_comment(&conn, comment_id)
    }

    pub fn resolve_thread(&self, thread_id: i64) -> Result<ThreadRow> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let changed = conn
            .execute(
                "UPDATE threads SET resolved = 1 WHERE id = ?1",
                params![thread_id],
            )
            .context("update thread")?;
        if changed == 0 {
            bail!("thread {thread_id} not found");
        }
        conn.query_row(
            "SELECT id, repo_path, base_ref, file_path, line, resolved, created_at
             FROM threads WHERE id = ?1",
            params![thread_id],
            map_thread,
        )
        .context("select resolved thread")
    }

    pub fn list_threads(
        &self,
        repo_path: &str,
        base_ref: &str,
        file_path: Option<&str>,
        only_unresolved: bool,
    ) -> Result<Vec<(ThreadRow, Vec<CommentRow>)>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut sql = String::from(
            "SELECT id, repo_path, base_ref, file_path, line, resolved, created_at
             FROM threads WHERE repo_path = ?1 AND base_ref = ?2",
        );
        if file_path.is_some() {
            sql.push_str(" AND file_path = ?3");
        }
        if only_unresolved {
            sql.push_str(" AND resolved = 0");
        }
        sql.push_str(" ORDER BY created_at ASC, id ASC");

        let mut stmt = conn.prepare(&sql).context("prepare list threads")?;
        let threads: Vec<ThreadRow> = match file_path {
            Some(p) => stmt
                .query_map(params![repo_path, base_ref, p], map_thread)?
                .collect::<Result<_, _>>()?,
            None => stmt
                .query_map(params![repo_path, base_ref], map_thread)?
                .collect::<Result<_, _>>()?,
        };

        let mut out = Vec::with_capacity(threads.len());
        for thread in threads {
            let comments = select_comments_for_thread(&conn, thread.id)?;
            out.push((thread, comments));
        }
        Ok(out)
    }
}

// -------- Row mappers --------

fn map_user(row: &Row<'_>) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn map_thread(row: &Row<'_>) -> rusqlite::Result<ThreadRow> {
    let resolved_int: i64 = row.get(5)?;
    Ok(ThreadRow {
        id: row.get(0)?,
        repo_path: row.get(1)?,
        base_ref: row.get(2)?,
        file_path: row.get(3)?,
        line: row.get(4)?,
        resolved: resolved_int != 0,
        created_at: row.get(6)?,
    })
}

fn select_comment(conn: &Connection, id: i64) -> Result<CommentRow> {
    conn.query_row(
        "SELECT c.id, c.thread_id, c.body, c.created_at,
                u.id, u.name, u.kind, u.created_at
         FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.id = ?1",
        params![id],
        |r| {
            Ok(CommentRow {
                id: r.get(0)?,
                thread_id: r.get(1)?,
                body: r.get(2)?,
                created_at: r.get(3)?,
                author: UserRow {
                    id: r.get(4)?,
                    name: r.get(5)?,
                    kind: r.get(6)?,
                    created_at: r.get(7)?,
                },
            })
        },
    )
    .with_context(|| format!("select comment {id}"))
}

fn select_comments_for_thread(conn: &Connection, thread_id: i64) -> Result<Vec<CommentRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.thread_id, c.body, c.created_at,
                    u.id, u.name, u.kind, u.created_at
             FROM comments c JOIN users u ON u.id = c.author_id
             WHERE c.thread_id = ?1
             ORDER BY c.created_at ASC, c.id ASC",
        )
        .context("prepare comments for thread")?;
    let rows = stmt.query_map(params![thread_id], |r| {
        Ok(CommentRow {
            id: r.get(0)?,
            thread_id: r.get(1)?,
            body: r.get(2)?,
            created_at: r.get(3)?,
            author: UserRow {
                id: r.get(4)?,
                name: r.get(5)?,
                kind: r.get(6)?,
                created_at: r.get(7)?,
            },
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

// -------- Tests --------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_comment_lifecycle() {
        let tmp = tempdir_free::TempDir::new();
        let db = Db::open_at(tmp.path().join("db.sqlite")).unwrap();

        let alice = db.upsert_user("alice", "user").unwrap();
        let alice_again = db.upsert_user("alice", "user").unwrap();
        assert_eq!(alice.id, alice_again.id, "upsert idempotent on (name,kind)");

        let (thread, first) = db
            .create_thread("/repo", "main", "src/main.rs", 10, alice.id, "nit")
            .unwrap();
        assert!(!thread.resolved);
        assert_eq!(first.body, "nit");
        assert_eq!(first.author.id, alice.id);

        let bot = db.upsert_user("claude", "agent").unwrap();
        let reply = db.append_comment(thread.id, bot.id, "ack").unwrap();
        assert_eq!(reply.thread_id, thread.id);
        assert_eq!(reply.author.kind, "agent");

        let resolved = db.resolve_thread(thread.id).unwrap();
        assert!(resolved.resolved);

        let listed = db.list_threads("/repo", "main", None, false).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].1.len(), 2);

        let unresolved_only = db.list_threads("/repo", "main", None, true).unwrap();
        assert!(unresolved_only.is_empty());
    }

    mod tempdir_free {
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        pub struct TempDir(pub PathBuf);
        impl TempDir {
            pub fn new() -> Self {
                let base = std::env::temp_dir()
                    .join(format!("lgtm-db-{}", std::process::id()))
                    .join(format!(
                        "{}",
                        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
                    ));
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
    }
}
