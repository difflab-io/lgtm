package storage

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/difflab-io/lgtm/internal/domain"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
)

func configs(t *testing.T) map[string]Config {
	result := map[string]Config{"sqlite": {Driver: "sqlite", DSN: filepath.Join(t.TempDir(), "lgtm.db")}}
	if dsn := os.Getenv("LGTM_TEST_POSTGRES_URL"); dsn != "" {
		result["postgres"] = Config{Driver: "postgres", DSN: dsn}
	}
	return result
}
func openTest(t *testing.T, c Config) *SQLStore {
	t.Helper()
	s, err := Open(context.Background(), c)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}
func read(t *testing.T, s Store, workspace string) domain.State {
	t.Helper()
	var out domain.State
	if err := s.Transact(context.Background(), workspace, false, func(state *domain.State) error { out = *state; return nil }); err != nil {
		t.Fatal(err)
	}
	return out
}
func local(id string) domain.Review {
	return domain.Review{ID: id, Kind: "local", Status: "draft", Actor: domain.Actor{Name: "sam", Kind: "human"}}
}
func TestSQLStorePersistenceIsolationTransactionsAndConstraints(t *testing.T) {
	for name, c := range configs(t) {
		t.Run(name, func(t *testing.T) {
			s := openTest(t, c)
			workspace := t.Name() + t.TempDir()
			ctx := context.Background()
			err := s.Transact(ctx, workspace, true, func(state *domain.State) error {
				state.Reviews = append(state.Reviews, local("review"))
				state.Comments = append(state.Comments, domain.Comment{ID: "root", ReviewID: "review", Path: "hello.go", Line: 2, Side: "RIGHT", Body: "Root\nSecond line", Actor: domain.Actor{Name: "Codex", Kind: "agent"}}, domain.Comment{ID: "reply", ThreadID: "root", ReviewID: "review", Path: "hello.go", Line: 2, Side: "RIGHT", Body: "Reply", Actor: domain.Actor{Name: "sam", Kind: "human"}})
				return nil
			})
			if err != nil {
				t.Fatal(err)
			}
			reopened := openTest(t, c)
			state := read(t, reopened, workspace)
			if len(state.Reviews) != 1 || len(state.Comments) != 2 || state.Comments[0].Actor.Kind != "agent" || state.Comments[1].ThreadID != "root" {
				t.Fatal(state)
			}
			if state = read(t, s, workspace+"-other"); len(state.Reviews) != 0 || len(state.Comments) != 0 {
				t.Fatal("workspace data leaked")
			}
			rejection := errors.New("abort")
			err = s.Transact(ctx, workspace, true, func(state *domain.State) error {
				state.Comments[0].Resolved = true
				state.Reviews = append(state.Reviews, local("discard"))
				return rejection
			})
			if !errors.Is(err, rejection) {
				t.Fatal(err)
			}
			state = read(t, s, workspace)
			if state.Comments[0].Resolved || len(state.Reviews) != 1 {
				t.Fatal("failed operation leaked changes")
			}
			err = s.Transact(ctx, workspace, true, func(state *domain.State) error {
				state.Reviews = append(state.Reviews, local("rollback"))
				state.Comments = append(state.Comments, domain.Comment{ID: "invalid", ReviewID: "missing", Path: "bad", Line: 1, Side: "RIGHT", Actor: domain.Actor{Name: "me", Kind: "human"}})
				return nil
			})
			if err == nil {
				t.Fatal("foreign key violation accepted")
			}
			if len(read(t, s, workspace).Reviews) != 1 {
				t.Fatal("SQL constraint failure partially committed")
			}
			// SQL uses entity rows rather than storing a JSON state blob.
			var rows int
			if err = s.db.QueryRowContext(ctx, s.query("SELECT COUNT(*) FROM comments WHERE workspace_id=?"), workspace).Scan(&rows); err != nil || rows != 2 {
				t.Fatal(rows, err)
			}
		})
	}
}
func TestIndependentConnectionsDoNotLoseUpdates(t *testing.T) {
	for name, c := range configs(t) {
		t.Run(name, func(t *testing.T) {
			first, second := openTest(t, c), openTest(t, c)
			workspace := t.Name() + t.TempDir()
			var wg sync.WaitGroup
			errs := make(chan error, 30)
			for i := 0; i < 30; i++ {
				wg.Add(1)
				go func(i int) {
					defer wg.Done()
					s := first
					if i%2 == 1 {
						s = second
					}
					errs <- s.Transact(context.Background(), workspace, true, func(state *domain.State) error {
						state.Reviews = append(state.Reviews, local(fmt.Sprintf("r-%d", i)))
						return nil
					})
				}(i)
			}
			wg.Wait()
			close(errs)
			for err := range errs {
				if err != nil {
					t.Fatal(err)
				}
			}
			if state := read(t, first, workspace); len(state.Reviews) != 30 {
				t.Fatal("lost concurrent writes", len(state.Reviews))
			}
		})
	}
}
func TestJSONImportIsAtomicIdempotentAndPreservesSource(t *testing.T) {
	for name, c := range configs(t) {
		t.Run(name, func(t *testing.T) {
			s := openTest(t, c)
			workspace := t.Name() + t.TempDir()
			source := filepath.Join(t.TempDir(), "state.json")
			legacy := domain.State{Reviews: []domain.Review{local("legacy")}, Comments: []domain.Comment{{ID: "note", Path: "README.md", Line: 1, Side: "RIGHT", Body: "Standalone", Actor: domain.Actor{Name: "Claude", Kind: "agent"}}}}
			data, _ := json.Marshal(legacy)
			if err := os.WriteFile(source, data, 0600); err != nil {
				t.Fatal(err)
			}
			for i := 0; i < 2; i++ {
				if err := s.ImportJSON(context.Background(), workspace, source); err != nil {
					t.Fatal(err)
				}
			}
			alias := filepath.Join(t.TempDir(), "legacy-alias.json")
			if err := os.Symlink(source, alias); err != nil {
				t.Fatal(err)
			}
			if err := s.ImportJSON(context.Background(), workspace, alias); err != nil {
				t.Fatalf("same source through a symlink must remain idempotent: %v", err)
			}
			if state := read(t, s, workspace); len(state.Reviews) != 1 || state.Reviews[0].ID != "legacy" || state.Comments[0].ReviewID != "" {
				t.Fatal(state)
			}
			unchanged, _ := os.ReadFile(source)
			if string(data) != string(unchanged) {
				t.Fatal("legacy source modified")
			}
			if err := os.WriteFile(source, append(data, ' '), 0600); err != nil {
				t.Fatal(err)
			}
			if err := s.ImportJSON(context.Background(), workspace, source); err == nil {
				t.Fatal("changed legacy state silently reimported")
			}
			if err := s.ImportJSON(context.Background(), workspace+"-invalid", source+"-missing"); err == nil {
				t.Fatal("missing import accepted")
			}
			// A final invalid comment must roll back earlier valid review insertion.
			legacy.Comments[0].ReviewID = "missing"
			data, _ = json.Marshal(legacy)
			os.WriteFile(source, data, 0600)
			if err := s.ImportJSON(context.Background(), workspace+"-invalid", source); err == nil {
				t.Fatal("invalid import accepted")
			}
			if len(read(t, s, workspace+"-invalid").Reviews) != 0 {
				t.Fatal("failed import partially persisted")
			}
		})
	}
}
func TestIndependentProcessesDoNotLoseUpdates(t *testing.T) {
	c := Config{Driver: "sqlite", DSN: filepath.Join(t.TempDir(), "process.db")}
	s := openTest(t, c)
	commands := []*exec.Cmd{}
	for i := 0; i < 2; i++ {
		cmd := exec.Command(os.Args[0], "-test.run=^TestStorageProcessHelper$")
		cmd.Env = append(os.Environ(), "LGTM_PROCESS_DB="+c.DSN, fmt.Sprintf("LGTM_PROCESS_ID=%d", i))
		if err := cmd.Start(); err != nil {
			t.Fatal(err)
		}
		commands = append(commands, cmd)
	}
	for _, cmd := range commands {
		if err := cmd.Wait(); err != nil {
			t.Fatal(err)
		}
	}
	if len(read(t, s, "processes").Reviews) != 20 {
		t.Fatal("cross-process updates lost")
	}
}
func TestStorageProcessHelper(t *testing.T) {
	path := os.Getenv("LGTM_PROCESS_DB")
	if path == "" {
		t.Skip("subprocess only")
	}
	s := openTest(t, Config{Driver: "sqlite", DSN: path})
	for i := 0; i < 10; i++ {
		if err := s.Transact(context.Background(), "processes", true, func(state *domain.State) error {
			state.Reviews = append(state.Reviews, local(fmt.Sprintf("%s-%d", os.Getenv("LGTM_PROCESS_ID"), i)))
			return nil
		}); err != nil {
			t.Fatal(err)
		}
	}
}
func TestMigrationChecksumAndNonSQLiteFileProtection(t *testing.T) {
	c := Config{Driver: "sqlite", DSN: filepath.Join(t.TempDir(), "db")}
	s := openTest(t, c)
	if _, err := s.db.Exec("UPDATE schema_migrations SET checksum='modified'"); err != nil {
		t.Fatal(err)
	}
	if changed, err := Open(context.Background(), c); err == nil {
		changed.Close()
		t.Fatal("edited migration history accepted")
	}
	legacy := filepath.Join(t.TempDir(), "legacy.json")
	os.WriteFile(legacy, []byte(`{"reviews":[]}`), 0600)
	if db, err := Open(context.Background(), Config{Driver: "sqlite", DSN: legacy}); err == nil {
		db.Close()
		t.Fatal("JSON source opened as SQLite")
	}
	data, _ := os.ReadFile(legacy)
	if string(data) != `{"reviews":[]}` {
		t.Fatal("source overwritten")
	}
}
