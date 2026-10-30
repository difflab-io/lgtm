# Development

Install Mise and Premise, then run from the repository root:

```sh
mise trust
mise install
premise run install
premise run dev
```

The backend listens on `127.0.0.1:8080`; Vite serves the frontend at `http://127.0.0.1:5173` and forwards `/rpc` to it. `LGTM_BACKEND_ADDR` changes the standalone listener, `LGTM_BACKEND_URL` changes Vite's backend target and `LGTM_WEB_PORT` changes the frontend development port. Run `premise run dev:backend` or `premise run dev:web` to start one side.

## Generated clients and bundling

Protobuf files under `apps/lgtm-backend/proto/` define the API. The backend build generates Rust bindings; `premise run //apps/lgtm-web:generate` generates TypeScript bindings for the greeting and hello-review services. Edit the schema rather than generated code.

`premise run build` builds the workspace and bundles backend/DB sources, generated protocol code and static web assets into the CLI. The publishable CLI must build without unpublished workspace packages. Generated output stays ignored; keep source scripts tracked.

```sh
premise run run:cli -- up --no-open
premise run run:cli -- review list --json
premise run run:cli -- ui --no-open
premise run run:cli -- down
```

`LGTM_HOME` isolates daemon PID, port and log files. The dummy DB package has no SQL features, migrations or database URL; old user database files are not read or removed. `LGTM_RPC_URL` can direct the CLI to an existing backend.

## Checks

```sh
premise run build
premise run test
premise run lint
premise run format:check
premise run e2e:fixtures:test
premise run e2e:smoke
```

During development, run only relevant package checks or filtered Vitest journeys. Run the pared-back `premise run e2e` for final validation or shared-harness changes. Default suites run offline; the retained recording/replay mechanism does not require live GitHub access for validation. See [E2E](../e2e/README.md).

Neovim development and released-plugin installation remain available through `premise run nvim:install-dev-plugin` and `premise run nvim:install-released-plugin`. The source-switch installer and E2E dependency lock retain pinned CodeDiff, WhichKey, notifications, NuiComponents/NUI and optional Kak.

## Distribution

The CLI is the publishable Rust package; backend and DB packages remain private. `premise run release:check` validates packaging without publishing. Release and registry publishing require separate authorization; the reset does not change those permissions or publish a release.
