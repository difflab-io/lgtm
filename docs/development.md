# Development Guide

From clone to a running local application. This project uses Mise for tools and tasks, Cargo for Rust dependencies, and Bun for frontend dependencies. [Premise](https://github.com/cloudvoyant/premise) provides workspace templates and CI.

## 1. Clone + install

Install [Mise](https://mise.jdx.dev/getting-started.html), then clone the repository:

```sh
git clone https://github.com/difflab-io/lgtm.git
cd lgtm
```

Check out the branch you want to develop before installing dependencies.

Install the workspace tools and dependencies:

```sh
mise trust
mise install
premise run install
```

The workspace pins Rust 1.94, Bun 1.4.2, and Buf 1.73.0. Buf generates code from Protobuf, the schema for RPC messages and services. Cargo and Bun use committed root lockfiles.

## 2. Local development

```sh
premise run dev
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
premise run //apps/lgtm-backend:dev
premise run //apps/lgtm-web:dev
```

The web task needs a running backend for RPC calls. The combined task starts both components.

## 3. CLI and libraries

```sh
premise run run:cli -- up --no-open
premise run run:cli -- review list --json
premise run run:cli -- ui --no-open
premise run run:cli -- status
premise run run:cli -- down
```

The `lgtm-cli` package builds the backend/DB source and web assets into the `lgtm` executable. `up` starts or reuses one background application on macOS/Linux, `status` reports its URLs, `ui` prints/opens its URL and `down` stops the identified process. `LGTM_HOME` isolates runtime files; `LGTM_RPC_URL` selects an existing RPC endpoint. Review list calls the hello demonstration; other domain operations are explicit unimplemented stubs.

### Neovim shell

The plugin retains loading entry points and pinned dependency/install/E2E machinery, including optional Kak; it makes no backend calls. See [plugin setup](../apps/lgtm.nvim/README.md) and [E2E](../e2e/README.md).

```sh
premise run nvim:install-dev-plugin
premise run nvim:install-released-plugin
```

## 4. Builds and generated code

```sh
premise run build
premise run //apps/lgtm-web:generate
premise run //apps/lgtm-backend:build
premise run //apps/lgtm-cli:bundle
```

The shared schemas live under `apps/lgtm-backend/proto/lgtm/v1/`: `hello.proto` defines the greeting API, `review.proto` defines the hello-review API. Backend build tasks regenerate the TypeScript client inside the web app, so no separate client package is needed.

The backend's `build.rs` generates Rust protocol code. The CLI bundle task builds the backend and static web app. It copies backend and database sources, generated Rust protocol files and web assets into `apps/lgtm-cli/bundle`. The bundled entry point is `bundle/backend/mod.rs`.

Git ignores that generated directory, but Cargo includes it in the CLI package. The CLI build script embeds the assets and compiles the copied backend source. Before a raw Cargo build in a fresh checkout, run the bundle task.

To call the standalone backend through Connect JSON, start it and send a greeting request:

```sh
curl --fail http://127.0.0.1:8080/lgtm.v1.GreeterService/SayHello \
  -H 'Content-Type: application/json' \
  --data '{"name":"World"}'
```

The response contains `Hello, World!`. The packaged CLI exposes the same method at `http://127.0.0.1:8080/rpc/lgtm.v1.GreeterService/SayHello` by default.

## 5. Checks

```sh
premise run test
premise run lint
premise run format:check
premise run e2e:fixtures:test
premise run e2e:smoke
```

Use project task addresses such as `premise run //apps/lgtm-backend:test` for focused checks. Run full E2E only for final validation or shared-fixture changes. The reduced suites cover hello-review, CLI safety, daemon/UI and Neovim loading; offline cassette tests preserve recorder/replay behavior. See [E2E](../e2e/README.md) for recording limits and commands.

### Database checks

```sh
premise run //libs/lgtm-db:test
premise run //apps/lgtm-backend:test
```

The DB package is a dummy data facade, with no SQL drivers, migrations or PostgreSQL matrix. Existing user databases remain untouched and unused. Backend and DB project tasks run their current public contracts through the tracked `test-rust-project` task.

## 6. CLI distribution

Only `lgtm-cli` publishes. The backend and database manifests set `publish = false`, and the Bun manifests set `private: true`. The installed executable includes the backend and web assets, so it needs no Bun, Node, Buf, or source checkout at runtime.

Make sure that the package and archive configuration are valid without uploading:

```sh
premise run release:check
mise exec -- goreleaser build --snapshot --single-target --clean --config .goreleaser.yaml
```

The archive configuration selects macOS and Linux for arm64 and x86_64. Build the full matrix on macOS with Xcode command-line tools. macOS targets use Apple's linker, and Linux targets use Zig.

To trigger a release candidate, put the exact marker `[publish-rc]` in the HEAD commit message and push the feature branch. Premise runs workspace checks, prepares the RC tag, and publishes eligible packages. Pull request events run checks without publishing.

The package job needs the `CRATES_TOKEN` organization or repository secret. Make sure that the registry contains the new version before reporting publication as complete.

Use `premise run release:cli` for an authorized GitHub archive release. Use `premise run publish:cli` for an authorized crates.io release. GitHub publication needs `GITHUB_TOKEN`, and direct Cargo publication needs `CARGO_REGISTRY_TOKEN` or `cargo login`.

Premise's current generic artifact selection skips monorepo archive publication. The explicit archive task handles that path. See the [architecture guide](architecture/index.md#packaging-flow) for the package boundaries.
