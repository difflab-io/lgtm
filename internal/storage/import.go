package storage

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/json"
	"fmt"
	"github.com/difflab-io/lgtm/internal/domain"
	"os"
	"path/filepath"
)

// ImportJSON performs an idempotent, transactional import into an empty workspace.
// The source file remains untouched. Changed sources and nonempty targets fail.
func (s *SQLStore) ImportJSON(ctx context.Context, workspace, source string) error {
	source, err := filepath.Abs(source)
	if err != nil {
		return err
	}
	source, err = filepath.EvalSymlinks(source)
	if err != nil {
		return err
	}
	data, err := os.ReadFile(source)
	if err != nil {
		return err
	}
	var legacy domain.State
	if err = json.Unmarshal(data, &legacy); err != nil {
		return fmt.Errorf("read legacy state: %w", err)
	}
	if legacy.Reviews == nil {
		legacy.Reviews = []domain.Review{}
	}
	if legacy.Comments == nil {
		legacy.Comments = []domain.Comment{}
	}
	checksum := fmt.Sprintf("%x", sha256.Sum256(data))
	return s.transact(ctx, workspace, true, func(tx *sql.Tx, state *domain.State) error {
		var previous string
		err := tx.QueryRowContext(ctx, s.query("SELECT checksum FROM legacy_imports WHERE workspace_id=? AND source=?"), workspace, source).Scan(&previous)
		if err == nil {
			if previous != checksum {
				return fmt.Errorf("legacy JSON changed after import; preserve both copies before reconciling")
			}
			return nil
		}
		if err != sql.ErrNoRows {
			return err
		}
		if len(state.Reviews) > 0 || len(state.Comments) > 0 {
			return fmt.Errorf("cannot import JSON into a nonempty workspace")
		}
		*state = legacy
		_, err = tx.ExecContext(ctx, s.query("INSERT INTO legacy_imports(workspace_id,source,checksum) VALUES(?,?,?)"), workspace, source, checksum)
		return err
	})
}
