package hosting

import (
	"flag"
	"github.com/difflab-io/lgtm/internal/storage"
	"os"
	"path/filepath"
)

// Flags shares database and listener configuration between the CLI and server.
type Flags struct{ Root, UI, DB, Driver, Workspace, HTTP, GRPC, Legacy *string }

func BindFlags(flags *flag.FlagSet) *Flags {
	return &Flags{
		Root: flags.String("repo", ".", "checkout root"), UI: flags.String("ui", "apps/lgtm-web-app/build", "SvelteKit static build"),
		Driver: flags.String("database", "sqlite", "database backend: sqlite or postgres"), DB: flags.String("db", "", "SQLite path or PostgreSQL DSN (defaults to LGTM_DATABASE_URL for postgres)"),
		Workspace: flags.String("workspace-id", "", "stable database workspace ID (defaults to canonical checkout path)"),
		HTTP:      flags.String("http", "127.0.0.1:9910", "HTTP listen address"), GRPC: flags.String("grpc", "127.0.0.1:9911", "gRPC listen address"),
		Legacy: flags.String("state", "", "legacy JSON state to import once, without changing its source"),
	}
}
func (f *Flags) Config() Config {
	dsn := *f.DB
	if dsn == "" {
		if *f.Driver == "postgres" {
			dsn = os.Getenv("LGTM_DATABASE_URL")
		} else {
			dsn = filepath.Join(*f.Root, ".lgtm/lgtm.db")
		}
	}
	return Config{Root: *f.Root, UI: *f.UI, GRPCAddr: *f.GRPC, HTTPAddr: *f.HTTP, Workspace: *f.Workspace, LegacyState: *f.Legacy, Database: storage.Config{Driver: *f.Driver, DSN: dsn}}
}
