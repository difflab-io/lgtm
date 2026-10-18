# Architecture

## Overview

LGTM runs a browser interface and Rust backend in one local process, managed by the `lgtm` CLI.

## Requirement

### Functional

- Support reviews through the web interface, Neovim, and the CLI directly.
- Support multiple reviews at the same time.
- Persist reviews across application restarts.
- Synchronize reviews with GitHub.
- Support notes and annotations without requiring a review.

### Non-Functional

- Keep large diffs responsive and readable.
- Make every interface action accessible through keyboard navigation.

## Design

### Project structure

```text
lgtm/
├── apps/
│   ├── lgtm-cli/       CLI and bundled application
│   ├── lgtm-backend/   Rust services and shared Protobuf schema
│   └── lgtm-web/       SvelteKit interface and generated client
└── libs/
    └── lgtm-db/        Private Rust library
```

### Components and Responsibilities

```mermaid
flowchart TD
    CLI["lgtm up / status / down"] -->|"Process management"| SERVER["Managed lgtm process"]
    BROWSER["Browser interface"] -->|"Static assets and /rpc"| SERVER
    NATIVE["Native gRPC client"] -->|"gRPC methods"| SERVER
    SERVER --> BACKEND["Bundled backend router"]
    BACKEND --> HELLO["Greeting and health services"]
    DB["lgtm-db: starter library"]
```

- `lgtm-cli` starts the application and serves the bundled interface and backend on the local machine.
- `lgtm-backend` provides greeting and health services through ConnectRPC. It also runs as a standalone server.
- `lgtm-web` sends browser requests through gRPC-Web at `/rpc/*`. Its client is generated from the backend schema.
- `lgtm-db` exports entity repositories with private SeaORM entities; backend review services own application models and workflows, with SQLite and optional PostgreSQL. Review persistence stays in the backend; only the current RPC router is bundled into the CLI. See the [database architecture](db/index.md).

### Data and Control Flow

#### Review flow

Each review has its own path and browser URL. Multiple browser tabs and editor clients share one local server. Opening another review does not replace existing reviews or start another server.

`lgtm review` starts or reuses the server and opens the dummy UI for the encoded path. It creates no review context/session and does not open a database. Session/comment commands are argument scaffolds pending backend review RPCs. Editor clients discover the server through `lgtm status --json`.

#### Development flow

```mermaid
flowchart TD
    DEV["mise run dev"] --> WATCH["Rust backend watcher"]
    DEV --> VITE["Vite: 127.0.0.1:5173"]
    WATCH --> GEN["Generate Rust and TypeScript protocol code"]
    GEN --> BACKEND["Standalone backend: 127.0.0.1:8080"]
    BROWSER["Browser"] --> VITE
    VITE -->|"Proxy /rpc requests"| BACKEND
```

`mise run dev` starts Vite and the Rust backend watcher. Vite updates the browser when frontend files change. Backend and schema changes regenerate client code and restart the server.

#### Packaging flow

```mermaid
flowchart TD
    SOURCE["Backend source and generated Rust protocols"] --> BUNDLE["CLI bundle directory"]
    WEB["SvelteKit static build"] --> BUNDLE
    BUNDLE --> PACKAGE["lgtm-cli Cargo package"]
    PACKAGE --> BINARY["lgtm executable"]
    BINARY --> ARCHIVE["macOS and Linux CLI archives"]
```

The CLI bundle contains backend source, generated Rust protocol code, and static web assets. Cargo includes the bundle in `lgtm-cli`. The installed executable runs without Bun, Buf, or a source checkout.

Only the CLI publishes. GoReleaser defines macOS and Linux archives for arm64 and x86_64. Premise publishes the Cargo package, while archive publication uses a separate task.

### Interfaces and Contracts

The [CLI architecture](cli/index.md) defines commands and server discovery. The shared [Protobuf schema](../../apps/lgtm-backend/proto/lgtm/v1/hello.proto) defines `GreeterService/SayHello`.

Native clients call `/lgtm.v1.GreeterService/SayHello`. Browser clients call the same method under `/rpc`. The greeting trims the supplied name and uses `World` when it is blank.

## Implementation

The backend uses Rust and ConnectRPC. Axum serves the bundled SvelteKit interface and routes its requests to the backend. Protobuf defines the shared service schema and generates Rust and TypeScript code.

The CLI embeds `daemonize` for background startup on macOS and Linux. `status --json` exposes server addresses for editor clients. `review` opens the current directory through the web interface's `path` query parameter.

The web app displays the requested path and calls the greeting service. The shared server serves static assets and backend methods through one loopback listener.

Mise manages tools and development tasks. Cargo and Bun manage dependencies. Tests cover native gRPC, browser requests, and CLI startup and shutdown.

## References

- [CLI module architecture](cli/index.md).
- [Review database architecture](db/index.md).
- [Review command architecture](cli/index.md).
- [Development guide](../development.md).
- [Architecture decision](../../adr/0001-initial-architecture.md).
- [Product requirements](../../prd/0001-core-concepts/README.md).
- [Design prototype](../../designs/prototypes/lgtm-v0/README.md).

See [backend review services](backend/index.md) for application models, public persistence calls and caller-owned transactions.
