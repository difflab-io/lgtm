# Development Guide

From clone to a running local application. This project uses Mise for tools and tasks, Cargo for Rust dependencies, and Bun for frontend dependencies. [Premise](https://github.com/cloudvoyant/premise) provides workspace templates and CI.

## 1. Clone + install

Install [Mise](https://mise.jdx.dev/getting-started.html), then clone the repository:

```sh
git clone https://github.com/difflab-io/lgtm.git
cd lgtm
```

If you are trying the unmerged scaffold, switch to its branch before installing dependencies:

```sh
git switch feature/scaffolding
```

Install the workspace tools and dependencies:

```sh
mise trust
mise install
mise run install
```

The workspace pins Rust 1.94, Bun 1.4.2, and Buf 1.73.0. Buf generates code from Protobuf, the schema for RPC messages and services. Cargo and Bun use committed root lockfiles.

## 2. Local development

```sh
mise run dev
```

Open `http://127.0.0.1:5173`. The command starts the Rust backend watcher and Vite, the frontend development server. Vite forwards `/rpc/*` requests to the backend at `http://127.0.0.1:8080`.

Vite updates the interface when frontend files change. Rust source, build-script, and Protobuf changes regenerate the web client and restart the backend. Ctrl-C stops both processes.

To change ports, set these environment variables before starting development:

<!-- prettier-ignore -->
| Variable            | Default                 |
| ------------------- | ----------------------- |
| `LGTM_BACKEND_ADDR` | `127.0.0.1:8080`        |
| `LGTM_BACKEND_URL`  | `http://127.0.0.1:8080` |
| `LGTM_WEB_PORT`     | `5173`                  |

`LGTM_BACKEND_ADDR` sets the backend listen address. `LGTM_BACKEND_URL` sets Vite's RPC target. `LGTM_WEB_PORT` sets Vite's listen port.

If you change the backend address, set the matching Vite target. If another process uses a required port, stop it or choose another port.

Individual development tasks are available:

```sh
mise run //apps/lgtm-backend:dev
mise run //apps/lgtm-web:dev
```

The web task needs a running backend for RPC calls. The combined task starts both components.

## 3. CLI and libraries

```sh
mise run //apps/lgtm-cli:run -- up --no-open
mise run //apps/lgtm-cli:run -- status
mise run //apps/lgtm-cli:run -- review
mise run //apps/lgtm-cli:run -- down
mise run //libs/lgtm-db:test
```

The Cargo package is named `lgtm-cli`, and its executable is named `lgtm`. Its run task builds the bundle before starting the CLI. The root `mise run run:cli -- up --no-open` task provides the same entry point.

`lgtm up` uses the embedded `daemonize` library on macOS and Linux. It starts one background process for the interface and RPC server. It opens a browser unless you pass `--no-open`. Repeated calls reuse the running instance.

`lgtm status` shows the UI, server, and browser RPC URLs. The UI and native API share one address. The browser API uses `/rpc` on that address.

`lgtm review` starts or reuses the server and opens `/?path=<encoded current directory>`. The interface displays the selected path. Use `lgtm review --no-open` to print the URL without opening a browser. If you need a different port for a new instance, pass `--port`.

Useful runtime options:

```sh
lgtm up --port 9000
lgtm up --foreground
LGTM_HOME=/tmp/lgtm-test lgtm up --port 0 --no-open
```

All runtime files live in `~/.lgtm`. The store module owns runtime paths and the PID/port files. The log module writes `server.log` with `tracing` and `tracing-subscriber`. Set `RUST_LOG` to change log filtering. `down` sends SIGTERM to the server, and Ctrl-C stops it in foreground mode. `LGTM_HOME` selects an isolated store for testing.

The database library provides persistent contexts, sessions and comments. See the
[database architecture](architecture/db/index.md) and [CLI examples](../apps/lgtm-cli/README.md).
SeaORM 2.0.4 uses SQLite by default and optional PostgreSQL. Database commands use
LGTM_HOME/reviews.sqlite or LGTM_DATABASE_URL; status does not open a database.
The browser still consumes only path and implements no review functionality.

### Neovim connection

The plugin can start the shared server and discover its addresses without opening a browser:

```sh
lgtm up --no-open
lgtm status --json
```

The discovery response contains `running`, `pid`, `ui_url`, `server_url`, and `rpc_url`. When the server is stopped, it returns `{"running":false}`. `/health` confirms that the shared UI/API server is ready.

Use Neovim's `vim.system` to run these commands asynchronously. Call backend methods through `server_url` using Connect JSON, as in the greeting example below. The web interface and plugin then use the same backend instead of separate servers.

The `path` query parameter selects a directory in the browser. It does not grant file access or create a review. The current backend exposes greeting and health services only.

## 4. Builds and generated code

```sh
mise run build
mise run //apps/lgtm-web:generate
mise run //apps/lgtm-backend:build
mise run //apps/lgtm-cli:bundle
```

The shared schema lives at `apps/lgtm-backend/proto/lgtm/v1/hello.proto`. It defines `lgtm.v1.GreeterService/SayHello`. Backend build tasks regenerate the TypeScript client inside the web app, so no separate client package is needed.

The backend's `build.rs` generates Rust protocol code. The CLI bundle task builds the backend and static web app. It copies only the greeting/health router, generated Rust protocol files and web assets into `apps/lgtm-cli/bundle`.

Git ignores that generated directory, but Cargo includes it in the CLI package. The CLI build script embeds the assets and compiles the copied backend source. Before a raw Cargo build in a fresh checkout, run the bundle task.

To call the standalone backend through Connect JSON, start it and send a greeting request:

```sh
curl --fail http://127.0.0.1:8080/lgtm.v1.GreeterService/SayHello \
  -H 'Content-Type: application/json' \
  --data '{"name":"World"}'
```

The response contains `Hello, World!`. The packaged CLI serves the same method at the same address by default.

## 5. Checks

```sh
mise run test
mise run lint
mise run format:check
```

Rust module tests live under `src/`, in the same source file’s `#[cfg(test)] mod tests`; shared fixtures live under `src/tests/`. Database migration and entity tests are colocated inside `lgtm-db`; they can access its private implementation without exporting it. Public integration tests live in each crate's `tests/` directory. Shared integration fixtures live in `tests/common/mod.rs`; the dummy forge is `tests/common/forge.rs` in the backend.

Tests cover native gRPC, browser RPC calls, daemon startup, instance reuse, shutdown, and occupied-port failures. Web integration tests start an isolated Rust backend. Lint runs Clippy, ESLint, and Svelte type checks.

To limit a check to one project, use its task path:

```sh
mise run //apps/lgtm-backend:test
mise run //apps/lgtm-web:lint
mise run //apps/lgtm-cli:test
```

Run `mise run format` to format application sources. Review formatting changes before committing them.

### Database checks

```sh
cargo test -p lgtm-db -p lgtm-backend
LGTM_TEST_POSTGRES_URL=postgres://USER@127.0.0.1:5432/lgtm_test mise run //libs/lgtm-db:test:postgres
cargo check -p lgtm-db --no-default-features --features postgres
```

Use a disposable PostgreSQL 17 database whose test user can create schemas.
Each shared test isolates its schema. All-feature tests require the URL and fail
rather than silently skip PostgreSQL. Premise invokes each project’s standard Mise test hook in the declared platform matrix. Linux DB/backend test hooks run SQLite/PostgreSQL contracts using an isolated PostgreSQL 17 Docker container (or an explicit LGTM_TEST_POSTGRES_URL); other/local invocations use SQLite unless a URL is supplied. Database checks are not a separate Actions job.

SeaORM and sea-orm-migration 2.0.4 use one shared ordered Rust migration history,
with SeaQuery 1.0.2 schema builders. Explicit transactions keep schema and
`seaql_migrations` updates atomic on both backends. The framework rejects unknown
applied migration names; it does not checksum source files. SQLite keeps foreign
keys enabled and the driver's five-second busy timeout, using one connection per
service pool and the default journal mode. A sibling migration lock serializes
SQLite startup. PostgreSQL deployments must run one migration job before parallel
application startup. Test-only upgrades cover populated data and failure recovery;
there are no older shipped schema upgrades or destructive migrations yet. Future
destructive SQLite changes need consistent backups and a table rebuild plan.

The CLI bundles no database source, drivers or migrations. Rerun the bundle task after greeting/health router or protocol changes before raw CLI Cargo checks.
Use cargo publish --dry-run (never an actual publish for validation) to compile the
package outside the workspace. Local/remote sync and desktop migration policy remain
[future ADR topics](../adr/README.md).

## 6. CLI distribution

Only `lgtm-cli` publishes. The backend and database manifests set `publish = false`, and the Bun manifests set `private: true`. The installed executable includes the backend and web assets, so it needs no Bun, Node, Buf, or source checkout at runtime.

Make sure that the package and archive configuration are valid without uploading:

```sh
mise run release:check
mise exec -- goreleaser build --snapshot --single-target --clean --config .goreleaser.yaml
```

The archive configuration selects macOS and Linux for arm64 and x86_64. Build the full matrix on macOS with Xcode command-line tools. macOS targets use Apple's linker, and Linux targets use Zig.

To trigger a release candidate, put the exact marker `[publish-rc]` in the HEAD commit message and push the feature branch. Premise runs workspace checks, prepares the RC tag, and publishes eligible packages. Pull request events run checks without publishing.

The package job needs the `CRATES_TOKEN` organization or repository secret. Make sure that the registry contains the new version before reporting publication as complete.

Use `mise run release:cli` for an authorized GitHub archive release. Use `mise run publish:cli` for an authorized crates.io release. GitHub publication needs `GITHUB_TOKEN`, and direct Cargo publication needs `CARGO_REGISTRY_TOKEN` or `cargo login`.

Premise's current generic artifact selection skips monorepo archive publication. The explicit archive task handles that path. See the [architecture guide](architecture/index.md#packaging-flow) for the package boundaries.
