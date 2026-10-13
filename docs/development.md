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

The workspace pins Rust 1.88, Bun 1.4.2, and Buf 1.73.0. Buf generates code from Protobuf, the schema for RPC messages and services. Cargo and Bun use committed root lockfiles.

## 2. Local development

```sh
mise run dev
```

Open `http://127.0.0.1:5173`. The command starts the Rust backend watcher and Vite, the frontend development server. Vite forwards `/rpc/*` requests to the backend at `http://127.0.0.1:8080`.

Vite updates the interface when frontend files change. Rust source, build-script, and Protobuf changes regenerate the web client and restart the backend. Ctrl-C stops both processes.

To change ports, set these environment variables before starting development:

| Variable            | Purpose                      | Default                 |
| ------------------- | ---------------------------- | ----------------------- |
| `LGTM_BACKEND_ADDR` | Backend listen address       | `127.0.0.1:8080`        |
| `LGTM_BACKEND_URL`  | Vite target for RPC requests | `http://127.0.0.1:8080` |
| `LGTM_WEB_PORT`     | Vite listen port             | `5173`                  |

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
mise run //apps/lgtm-cli:run -- down
mise run //libs/lgtm-db:test
```

The Cargo package is named `lgtm-cli`, and its executable is named `lgtm`. Its run task builds the bundle before starting the CLI. The root `mise run run:cli -- up --no-open` task provides the same entry point.

`lgtm up` starts one managed background process for the embedded interface and RPC server. It opens a browser unless you pass `--no-open`. Repeated calls reuse the running instance.

Useful runtime options:

```sh
lgtm up --port 9000
lgtm up --foreground
lgtm --state-dir /path/to/state up --no-open
```

State and `server.log` live in the platform's local state directory under `lgtm`. `LGTM_STATE_DIR` or `--state-dir` overrides that directory. In foreground mode, Ctrl-C stops the server.

The database library contains starter behavior. No database or storage integration exists yet.

## 4. Builds and generated code

```sh
mise run build
mise run //apps/lgtm-web:generate
mise run //apps/lgtm-backend:build
mise run //apps/lgtm-cli:bundle
```

The shared schema lives at `apps/lgtm-backend/proto/lgtm/v1/hello.proto`. It defines `lgtm.v1.GreeterService/SayHello`. Backend build tasks regenerate the TypeScript client inside the web app, so no separate client package is needed.

The backend's `build.rs` generates Rust protocol code. The CLI bundle task builds the backend and static web app. It copies backend implementation source, generated Rust protocol files, and web assets into `apps/lgtm-cli/bundle`.

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

Tests cover native gRPC, browser RPC calls, daemon startup, instance reuse, shutdown, and occupied-port failures. Web integration tests start an isolated Rust backend. Lint runs Clippy, ESLint, and Svelte type checks.

To limit a check to one project, use its task path:

```sh
mise run //apps/lgtm-backend:test
mise run //apps/lgtm-web:lint
mise run //apps/lgtm-cli:test
```

Run `mise run format` to format application sources. Review formatting changes before committing them.

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
