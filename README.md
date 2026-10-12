# lgtm

LGTM is a local code review tool with a Rust gRPC backend and SvelteKit UI. The current UI demonstrates a hello-world RPC; review features and database storage come later.

| Path | Purpose | Publication |
| --- | --- | --- |
| `apps/lgtm-cli` | The `lgtm` executable, bundled UI, and managed server | crates.io and CLI archives |
| `apps/lgtm-backend` | Private Rust library and standalone gRPC server | Included inside the CLI; no separate package |
| `apps/lgtm-web` | Private SvelteKit app and generated RPC client | Static assets included inside the CLI |
| `libs/lgtm-db` | Minimal private Rust library; no database yet | None |

## Installed CLI

```sh
cargo install lgtm-cli
lgtm up
lgtm status
lgtm down
```

The executable is named `lgtm`; the Cargo package is `lgtm-cli`. `lgtm up` starts one managed background process serving both the embedded UI and the gRPC API at `http://127.0.0.1:8080`, then opens a browser. The same process serves the web app's `/rpc/*` gRPC-Web calls and native gRPC endpoints. No Bun, Node, Buf, Pitchfork, backend binary, or source checkout is needed at runtime.

Useful options:

```sh
lgtm up --no-open
lgtm up --port 9000
lgtm up --foreground
lgtm --state-dir /path/to/state up
```

A repeated `up` reuses the running instance. `down` stops both UI and RPC serving. State and `server.log` live in the platform's local state directory under `lgtm`; `LGTM_STATE_DIR` or `--state-dir` overrides it. In foreground mode, Ctrl-C stops the server. The managed process is bound to loopback.

## Local development

Install [mise](https://mise.jdx.dev/), then run from the checkout:

```sh
mise trust
mise install
mise run install
mise run dev
```

`mise run dev` starts the Rust backend watcher and Vite together. Open `http://127.0.0.1:5173`. Vite forwards `/rpc/*` to the backend at `http://127.0.0.1:8080`. Rust, build-script, and Protobuf changes regenerate the client and restart the backend; Vite updates the UI. Ctrl-C stops both processes.

`LGTM_BACKEND_ADDR` sets the backend listen address, `LGTM_BACKEND_URL` sets Vite's RPC target, and `LGTM_WEB_PORT` changes Vite's port. Individual tasks remain available:

```sh
mise run //apps/lgtm-backend:dev
mise run //apps/lgtm-web:dev
mise run //apps/lgtm-cli:run -- up --no-open
```

The CLI run task first builds its bundle and then runs the packaged application. Use the combined dev task for live development.

## Builds, generation, and checks

```sh
mise run build
mise run test
mise run lint
mise run format:check
```

Rust 1.88, Bun 1.4.2, and Buf 1.73.0 are pinned. Root Cargo and Bun lockfiles are committed. `mise run format` formats application sources.

`apps/lgtm-backend/proto/lgtm/v1/hello.proto` defines `lgtm.v1.GreeterService/SayHello`. Backend builds and dev restarts regenerate the TypeScript client inside `lgtm-web`; no separate client package is needed. Rust code generation runs in the private backend's `build.rs`.

```sh
mise run //apps/lgtm-web:generate
mise run //apps/lgtm-backend:build
mise run //apps/lgtm-cli:bundle
```

The CLI bundle task builds the private backend and static UI, then copies backend implementation source, generated Rust protocol files, and web assets into `apps/lgtm-cli/bundle`. That generated directory is ignored by Git and explicitly included by Cargo's package manifest. The CLI's own build script uses only Rust and those files, so its crates.io archive builds independently of the monorepo. A raw Cargo build from a fresh checkout needs the bundle task first.

Tests cover native gRPC, gRPC-Web, real TypeScript-client calls, daemon startup/reuse/shutdown, and occupied-port failure. Web integration tests start an isolated Rust backend.

Call the service using Connect JSON:

```sh
curl --fail http://127.0.0.1:8080/lgtm.v1.GreeterService/SayHello \
  -H 'Content-Type: application/json' \
  --data '{"name":"World"}'
```

## CLI release

Only `lgtm-cli` is publishable. Backend and database manifests set `publish = false`, and Bun manifests set `private: true`. Backend implementation and UI assets ship inside the CLI package and executable. Publication tasks regenerate the bundle before uploading. The explicit `.goreleaser.yaml` selects only the `lgtm` executable for macOS and Linux on arm64 and x86_64. Build the full archive matrix on macOS with Xcode command-line tools installed; macOS targets use Apple's linker and Linux targets use Zig.

```sh
# Verify packaging without uploading.
mise run release:check

# Build one local CLI archive executable without publishing.
mise exec -- goreleaser build --snapshot --single-target --clean --config .goreleaser.yaml
```

For an RC, put the exact marker `[publish-rc]` in the HEAD commit message and push the feature branch. The Premise on-commit workflow validates the workspace, prepares the RC tag, and publishes eligible packages. Pull request events do not publish. The `CRATES_TOKEN` organization or repository secret must be available to the package job. Verify package publication before claiming an RC is published.

Use `mise run release:cli` for an authorized GitHub archive release and `mise run publish:cli` for an authorized crates.io release. GitHub requires `GITHUB_TOKEN`; direct Cargo publication requires `CARGO_REGISTRY_TOKEN` or `cargo login`. Premise's current generic artifact selection skips monorepo archive publication, so the explicit archive task handles that path.

## Template sources and project documents

Generated with `~/Projects/premise` and the local `premise-cargo` and `premise-bun` registries. `premise.yaml` records their portable official selectors. The private backend began with the Rust library template and was moved into `apps/`; the CLI uses the Clap template and the web app uses SvelteKit.

- [Architecture decision records](adr/README.md)
- [Product requirements documents](prd/README.md)
- [Web UI design and snapshots](designs/prototypes/lgtm-v0/README.md)
