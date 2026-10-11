// Package storage persists review entities with the same transaction contract
// on SQLite and PostgreSQL. The review domain has no driver-specific SQL.
package storage

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"embed"
	"fmt"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/difflab-io/lgtm/internal/domain"
	_ "github.com/jackc/pgx/v5/stdlib"
	_ "modernc.org/sqlite"
)

// Store provides an isolated workspace snapshot for each operation. Mutations
// lock the workspace in the database before loading and commit as one transaction.
type Store interface {
	Transact(context.Context, string, bool, func(*domain.State) error) error
	Close() error
	Ping(context.Context) error
}
type Config struct{ Driver, DSN string }
type SQLStore struct {
	db      *sql.DB
	dialect string
}

//go:embed migrations/*.sql
var migrations embed.FS

func Open(ctx context.Context, config Config) (*SQLStore, error) {
	driver := config.Driver
	if driver == "" {
		driver = "sqlite"
	}
	dsn := config.DSN
	switch driver {
	case "sqlite":
		if dsn == "" {
			return nil, fmt.Errorf("SQLite database path required")
		}
		if !strings.HasPrefix(dsn, "file:") && dsn != ":memory:" {
			path, err := filepath.Abs(dsn)
			if err != nil {
				return nil, err
			}
			if err = os.MkdirAll(filepath.Dir(path), 0700); err != nil {
				return nil, err
			}
			if existing, err := os.Open(path); err == nil {
				header := make([]byte, 16)
				n, readErr := io.ReadFull(existing, header)
				existing.Close()
				if n > 0 && (readErr != nil || string(header) != "SQLite format 3\x00") {
					return nil, fmt.Errorf("%s is not a SQLite database; import JSON with lgtm migrate", path)
				}
			} else if !os.IsNotExist(err) {
				return nil, err
			}

			file, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
			if err != nil {
				return nil, err
			}
			file.Close()
			dsn = (&url.URL{Scheme: "file", Path: path}).String()
		}
		separator := "?"
		if strings.Contains(dsn, "?") {
			separator = "&"
		}
		dsn += separator + "_pragma=foreign_keys(1)&_pragma=busy_timeout(30000)&_pragma=journal_mode(WAL)&_txlock=immediate"
	case "postgres":
		if dsn == "" {
			return nil, fmt.Errorf("PostgreSQL DSN required")
		}
	default:
		return nil, fmt.Errorf("database driver must be sqlite or postgres")
	}
	sqlDriver := driver
	if driver == "postgres" {
		sqlDriver = "pgx"
	}
	db, err := sql.Open(sqlDriver, dsn)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(8)
	db.SetMaxIdleConns(8)
	if strings.HasPrefix(config.DSN, ":memory:") {
		db.SetMaxOpenConns(1)
	}
	s := &SQLStore{db: db, dialect: driver}
	if err = db.PingContext(ctx); err == nil {
		err = s.migrate(ctx)
	}
	if err != nil {
		db.Close()
		return nil, err
	}
	return s, nil
}
func (s *SQLStore) Close() error                   { return s.db.Close() }
func (s *SQLStore) Ping(ctx context.Context) error { return s.db.PingContext(ctx) }
func (s *SQLStore) query(query string) string {
	if s.dialect != "postgres" {
		return query
	}
	i := 0
	return replaceParameters(query, &i)
}
func replaceParameters(query string, i *int) string {
	var b strings.Builder
	for _, r := range query {
		if r == '?' {
			*i++
			fmt.Fprintf(&b, "$%d", *i)
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}
func (s *SQLStore) migrate(ctx context.Context) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if s.dialect == "postgres" {
		if _, err = tx.ExecContext(ctx, "SELECT pg_advisory_xact_lock(763247011)"); err != nil {
			return err
		}
	}
	if _, err = tx.ExecContext(ctx, "CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY, checksum TEXT NOT NULL)"); err != nil {
		return err
	}
	entries, err := migrations.ReadDir("migrations")
	if err != nil {
		return err
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Name() < entries[j].Name() })
	for _, entry := range entries {
		data, err := migrations.ReadFile("migrations/" + entry.Name())
		if err != nil {
			return err
		}
		checksum := fmt.Sprintf("%x", sha256.Sum256(data))
		var existing string
		err = tx.QueryRowContext(ctx, s.query("SELECT checksum FROM schema_migrations WHERE version=?"), entry.Name()).Scan(&existing)
		if err == nil {
			if existing != checksum {
				return fmt.Errorf("migration checksum changed: %s", entry.Name())
			}
			continue
		}
		if err != sql.ErrNoRows {
			return err
		}
		for _, statement := range strings.Split(string(data), ";") {
			if strings.TrimSpace(statement) != "" {
				if _, err = tx.ExecContext(ctx, statement); err != nil {
					return fmt.Errorf("migration %s: %w", entry.Name(), err)
				}
			}
		}
		if _, err = tx.ExecContext(ctx, s.query("INSERT INTO schema_migrations(version,checksum) VALUES(?,?)"), entry.Name(), checksum); err != nil {
			return err
		}
	}
	return tx.Commit()
}
func (s *SQLStore) Transact(ctx context.Context, workspace string, write bool, fn func(*domain.State) error) error {
	return s.transact(ctx, workspace, write, func(_ *sql.Tx, state *domain.State) error { return fn(state) })
}
func (s *SQLStore) transact(ctx context.Context, workspace string, write bool, fn func(*sql.Tx, *domain.State) error) error {
	if workspace == "" {
		return fmt.Errorf("workspace ID required")
	}
	// Workspace registration is separate so concurrent readers never upgrade an
	// existing SQLite snapshot to a write transaction.
	if _, err := s.db.ExecContext(ctx, s.query("INSERT INTO workspaces(id) VALUES(?) ON CONFLICT(id) DO NOTHING"), workspace); err != nil {
		return err
	}
	tx, err := s.db.BeginTx(ctx, &sql.TxOptions{ReadOnly: !write, Isolation: isolation(write, s.dialect)})
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if write {
		if _, err = tx.ExecContext(ctx, s.query("UPDATE workspaces SET revision=revision+1 WHERE id=?"), workspace); err != nil {
			return err
		}
	}
	state, err := s.load(ctx, tx, workspace)
	if err != nil {
		return err
	}
	if err = fn(tx, &state); err != nil {
		return err
	}
	if write {
		if err = s.save(ctx, tx, workspace, state); err != nil {
			return err
		}
	}
	return tx.Commit()
}
func (s *SQLStore) load(ctx context.Context, tx *sql.Tx, workspace string) (domain.State, error) {
	state := domain.State{Reviews: []domain.Review{}, Comments: []domain.Comment{}}
	rows, err := tx.QueryContext(ctx, s.query("SELECT id,kind,repository,pr,head,status,body,actor_name,actor_kind,remote_id FROM reviews WHERE workspace_id=? ORDER BY position"), workspace)
	if err != nil {
		return state, err
	}
	for rows.Next() {
		var r domain.Review
		if err = rows.Scan(&r.ID, &r.Kind, &r.Repo, &r.PR, &r.Head, &r.Status, &r.Body, &r.Actor.Name, &r.Actor.Kind, &r.RemoteID); err != nil {
			rows.Close()
			return state, err
		}
		state.Reviews = append(state.Reviews, r)
	}
	err = rows.Err()
	rows.Close()
	if err != nil {
		return state, err
	}
	rows, err = tx.QueryContext(ctx, s.query("SELECT id,COALESCE(thread_id,''),COALESCE(review_id,''),resolved,path,line,side,body,actor_name,actor_kind FROM comments WHERE workspace_id=? ORDER BY position"), workspace)
	if err != nil {
		return state, err
	}
	defer rows.Close()
	for rows.Next() {
		var c domain.Comment
		if err = rows.Scan(&c.ID, &c.ThreadID, &c.ReviewID, &c.Resolved, &c.Path, &c.Line, &c.Side, &c.Body, &c.Actor.Name, &c.Actor.Kind); err != nil {
			return state, err
		}
		state.Comments = append(state.Comments, c)
	}
	return state, rows.Err()
}
func nullable(s string) any {
	if s == "" {
		return nil
	}
	return s
}
func (s *SQLStore) save(ctx context.Context, tx *sql.Tx, workspace string, state domain.State) error {
	for i, r := range state.Reviews {
		_, err := tx.ExecContext(ctx, s.query(`INSERT INTO reviews(workspace_id,id,position,kind,repository,pr,head,status,body,actor_name,actor_kind,remote_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(workspace_id,id) DO UPDATE SET status=excluded.status,body=excluded.body,remote_id=excluded.remote_id`), workspace, r.ID, i, r.Kind, r.Repo, r.PR, r.Head, r.Status, r.Body, r.Actor.Name, r.Actor.Kind, r.RemoteID)
		if err != nil {
			return err
		}
	}
	for i, c := range state.Comments {
		_, err := tx.ExecContext(ctx, s.query(`INSERT INTO comments(workspace_id,id,position,thread_id,review_id,resolved,path,line,side,body,actor_name,actor_kind) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(workspace_id,id) DO UPDATE SET review_id=excluded.review_id,resolved=excluded.resolved,body=excluded.body`), workspace, c.ID, i, nullable(c.ThreadID), nullable(c.ReviewID), c.Resolved, c.Path, c.Line, c.Side, c.Body, c.Actor.Name, c.Actor.Kind)
		if err != nil {
			return err
		}
	}
	return nil
}

func isolation(write bool, dialect string) sql.IsolationLevel {
	if !write && dialect == "postgres" {
		return sql.LevelRepeatableRead
	}
	return sql.LevelDefault
}
