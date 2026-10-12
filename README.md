# lgtm

LGTM code review workspace. This scaffold provides runnable applications; review features and database storage come later.

| Path | Purpose | Publication |
| --- | --- | --- |
| `apps/lgtm-cli` | Rust Clap CLI | crates.io and CLI archives |
| `apps/lgtm-backend` | Rust library with a Connect RPC server binary | Private |
| `apps/lgtm-web` | Bun SvelteKit application | Private |
| `libs/lgtm-db` | Minimal Rust library; no database yet | Private |

## Setup and checks

Install [mise](https://mise.jdx.dev/), then run from the repository root:

```sh
mise trust
mise install
mise run install
mise run build
mise run test
mise run lint
mise run format:check
```

Rust 1.88 and Bun 1.4.2 are pinned in `mise.toml`. Commit both root lockfiles when committing the scaffold. Run `mise run format` to format the application sources. Per-project tasks are also available, for example `mise run //apps/lgtm-web:build`.

## Run

```sh
mise run run:cli -- --help
mise run run:cli -- --version
mise run //apps/lgtm-backend:dev
```

The backend listens on `127.0.0.1:8080`. Set `LGTM_BACKEND_ADDR` to change it. Ctrl-C marks health as not serving and shuts down gracefully. Its greeting and standard health services accept Connect, gRPC, and gRPC-Web clients.

From another terminal, call the Connect JSON health endpoint:

```sh
curl --fail http://127.0.0.1:8080/grpc.health.v1.Health/Check \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  --data '{"service":""}'
```

Start the web app in another terminal:

```sh
mise run //apps/lgtm-web:dev
```

Open the local URL printed by Vite, enter a name, and click **Say hello**. The browser uses the generated gRPC-Web client in `apps/lgtm-web/src/lib/gen`. SvelteKit forwards `/rpc/*` requests to the Rust backend. Set `LGTM_BACKEND_URL` when starting the web app if the backend is not at `http://127.0.0.1:8080`. The browser and proxy share an origin, so no backend CORS configuration or separate TypeScript client package is needed.

To preview the production build:

```sh
mise exec -- bun run --cwd apps/lgtm-web build
mise exec -- bun run --cwd apps/lgtm-web start
```

## Protobuf generation

`apps/lgtm-backend/proto/lgtm/v1/hello.proto` defines `lgtm.v1.GreeterService/SayHello`. Both Rust and TypeScript derive their API from that file. Buf 1.73.0 is pinned in Mise; the Protobuf ES plugin is pinned in the web app. Rust generation runs in `build.rs`, with output in Cargo's build directory. The TypeScript client is generated into the web app and checked in.

```sh
# Regenerate the web client, then compile the backend.
mise run //apps/lgtm-backend:build

# Regenerate only the web client.
mise run //apps/lgtm-web:generate
```

The aggregate `mise run build` also regenerates the web client first. Both per-project `dev` tasks regenerate the client before startup, as do the root dev shortcuts. After editing the schema during a session, restart the backend dev task to regenerate both clients; Vite reloads the changed TypeScript client. Direct Cargo commands regenerate Rust types when the schema changes; use the Mise backend build to update TypeScript as well. Generated TypeScript files are excluded from formatting and lint fixes.

The greeting can also be called using Connect JSON:

```sh
curl --fail http://127.0.0.1:8080/lgtm.v1.GreeterService/SayHello \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  --data '{"name":"World"}'
```

`mise run //apps/lgtm-web:test` compiles and starts an isolated Rust backend to test the generated gRPC-Web client. The Rust integration tests cover native gRPC and gRPC-Web greeting requests.

## CLI release

Only `lgtm-cli` is publishable. Backend and database manifests set `publish = false`; both Bun manifests set `private: true`. Private Rust publication tasks do nothing. The explicit `.goreleaser.yaml` builds and archives only `lgtm-cli`, for macOS and Linux on arm64 and x86_64.

Validate without uploading:

```sh
mise run release:check
mise exec -- goreleaser build --snapshot --single-target --clean --config .goreleaser.yaml
```

For an authorized release, update the CLI version and lockfile, commit and tag that version, then use `mise run release:cli` for GitHub archives and `mise run publish:cli` for crates.io. GitHub requires `GITHUB_TOKEN`; Cargo requires `CARGO_REGISTRY_TOKEN` or `cargo login`. These commands explicitly select the CLI. No backend or web deployment is configured.

For an RC, put the exact marker `[publish-rc]` in the HEAD commit message and push the feature branch. The Premise `on-commit` GitHub workflow plans the prerelease version, validates the projects, creates the GitHub prerelease, then publishes eligible packages. Pull request events never publish. Configure the repository Actions secret `CRATES_TOKEN` for crates.io; the workflow passes it only to the package job. Check that both the release and package jobs succeed before claiming the RC was published. The Cargo backend sets and restores the prerelease version during publication, so a manual manifest version change is not required for this trigger.

Use these explicit release tasks for this scaffold. The locally installed Premise build currently fails `pm release --dry-run` in this linked Git worktree with `read release HEAD: reference not found`; it also derives generic Cargo archive configuration from registry templates rather than this monorepo's project list.

## Template sources

Generated with `~/Projects/premise` and the local `premise-cargo` and `premise-bun` registries. The installed Premise binary was updated from local source to support current registry CI fields. `premise.yaml` records the corresponding portable official selectors:

- `cloudvoyant/premise-cargo:premise-clap-cli` for the CLI.
- `cloudvoyant/premise-cargo:premise-rust-lib` for backend and database. The backend was moved from the generated `libs/` location to `apps/` and its manifest path updated.
- `cloudvoyant/premise-bun:premise-sveltekit-app` for the web app. Generation used a clean tracked registry snapshot to exclude installed dependencies.

## Project documents

- [Architecture decision records](adr/README.md)
- [Product requirements documents](prd/README.md)
- [Web UI design and snapshots](designs/prototypes/lgtm-v0/README.md)
