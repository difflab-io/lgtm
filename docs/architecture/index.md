# LGTM — software architecture

## Overview

LGTM is a code review and workspace platform for humans and AI agents. Users review local uncommitted changes or remote GitHub pull requests, edit files in a desktop IDE with live language server diagnostics, and run project tasks without modifying tracked repository configuration.

## Requirement

### Functional

- Stage, commit, push, review diffs, and manage comments on local worktree snapshots (`--local`) and remote GitHub pull requests.
- Attribute sessions and comments to human users or AI agents (`--as <agent>`), support concurrent draft sessions on the same review, and attach AI disclosure banners on remote pull requests.
- Open, browse, create, edit, rename, and delete workspace files in a native desktop IDE with live diagnostics, hover documentation, and go-to-definition.
- Discover workspace languages, install missing `mise` and language server tools automatically, and run project tasks through `lgtm run`.

### Non-Functional

- Keep `--local` reviews non-destructive so local staging and shadow commits never modify the working checkout's `.git/index`, `HEAD`, `.gitignore`, or root `mise.toml`.
- Keep all local review, file editing, and cached toolchain workflows working offline.
- Save remote pull request draft sessions and comments locally before syncing to GitHub so network failures never lose review work.
- Restrict workspace file operations to the active workspace root and keep UI interactions responsive while language servers initialize in the background.

## Design

### Usage

```sh
# 1. Start the background RPC daemon and run local or remote PR review workflows
lgtm up
lgtm add src/ --local
lgtm commit --local -m "Snapshot worktree"
lgtm review new --local
lgtm comment new "Extract helper" --file src/lib.rs --line 24 --local --as claude-sonnet-4-5
lgtm review show --local --group-by file --unresolved
lgtm review close --local --with approve --body "Ready"

# 2. List and execute workspace tasks synthesized in <workspace>/.lgtm/mise.toml
lgtm run --list
lgtm run check

# 3. Launch the native Tauri desktop IDE at a workspace directory
lgtm open .
premise run dev:desktop
```

### Components and Responsibilities

```mermaid
flowchart TB
    subgraph Apps["Applications & Services (apps/)"]
        CLI["CLI Client (lgtm-cli)\nCLI commands, daemon, lgtm run"]
        FORGE["Forge Backend (lgtm-forge)\nConnect RPC server, ReviewService, SnapshotStore, RemoteForge"]
        subgraph DesktopApp["Desktop Application"]
            WEB["Web Frontend (lgtm-web)\nSvelte 5 IDE shell & reactive services"]
            DESKTOP["Rust Backend (lgtm-desktop)\nTauri v2 host, WorkspaceService, LspService"]
            WEB <-->|"Tauri IPC + lsp-status / lsp-diagnostics"| DESKTOP
        end
    end

    subgraph Libraries["Shared Libraries (libs/)"]
        UI["lgtm-ui\nHelical UI, Ark UI & CodeMirror 6 components"]
        SESS["lgtm-session\nSessionService & ~/.lgtm/sessions/default.json"]
        PROJ["lgtm-project\nProjectService, Project & Worktree"]
        RSU["lgtm-rsutils\ngit, paths, slugs & url"]
        DB["lgtm-db\nSeaORM repositories & transactions"]
        LSP["lgtm-lsp\nLgtmLspManager, LspCacheService, stdio JSON-RPC"]
        ENV["lgtm-env\nMiseService & .lgtm/mise.toml synthesis"]
        CFG["lgtm-config\nConfigService, TOML presets & language discovery"]
    end

    subgraph Storage["Local & External Systems"]
        SQLITE[("~/.lgtm/lgtm.db\nSQLite / PostgreSQL")]
        SNAP[("~/.lgtm/snapshots/$repoSlug\ngitoxide bare repo")]
        GIT[("Working Checkout (.git)\n& GitHub REST/GraphQL API")]
        MISE_BIN[("~/.lgtm/bin/mise &\n<workspace>/.lgtm/mise.toml")]
        LSP_PROC[("Language Server Processes\nrust-analyzer, tsserver, pyright, gopls, marksman")]
    end

    WEB --> UI
    CLI -->|"Connect RPC (/rpc)"| FORGE
    CLI -->|"lgtm run"| ENV
    DESKTOP --> SESS
    SESS --> PROJ
    PROJ --> RSU
    SESS --> RSU
    FORGE --> RSU
    DESKTOP --> LSP
    DESKTOP --> ENV
    DESKTOP --> CFG
    FORGE --> DB
    FORGE -->|"--local (gitoxide)"| SNAP
    FORGE -->|"ForgeBacked (git + octocrab)"| GIT
    DB --> SQLITE
    LSP --> ENV
    LSP --> CFG
    LSP <-->|"stdio JSON-RPC 2.0"| LSP_PROC
    ENV --> CFG
    ENV --> MISE_BIN
```

- [`lgtm-cli`](cli/index.md) (`apps/lgtm-cli`): Parses CLI commands, manages the background `lgtm-forge` RPC daemon (`up`, `down`, `status`, `serve`), launches the `lgtm-desktop` IDE (`open [<path>]`), proxies review/VCS commands (`add`, `commit`, `push`, `review`, `comment`) to `lgtm-forge` over Connect RPC, and delegates `lgtm run` to `lgtm-env`.
- [`lgtm-forge`](forge/index.md) (`apps/lgtm-forge`): Serves `lgtm.forge.v1.ReviewService` over Connect RPC, orchestrating `ReviewService` across `lgtm-db`, `SnapshotStore` (`extensions::gitoxide` for `--local` reviews), native `git` CLI checkout commands, and `RemoteForge` (`GitHubForge` via `extensions::octocrab`).
- [Desktop Application](desktop/index.md) (`apps/lgtm-web` + `apps/lgtm-desktop`):
  - **Web Frontend (`lgtm-web`)**: Reactive Svelte 5 IDE shell (`Shell.svelte`, `WorkspaceExplorer.svelte`, `ContentPane.svelte`, `LspStatusMenu.svelte`) and `$state` services (`WorkspaceService`, `LspService`) communicating with the Rust host via `TauriWorkspaceClient`.
  - **Rust Backend (`lgtm-desktop`)**: Native Tauri v2 host owning `WorkspaceService` (sandboxed filesystem CRUD) and desktop `LspService` (bridging `lgtm-lsp` events to `lsp-status` and `lsp-diagnostics` Tauri events and coordinating the quit handshake).
- `lgtm-ui` (`libs/lgtm-ui`): Provides reusable, app-agnostic Svelte 5 + `@ark-ui/svelte` + CodeMirror 6 components (`CodeEditor`, `TreeView`, `TabBar`, `MenuBar`, `ContextMenu`, `MarkdownPreview`, `Sidebar`) styled with semantic Tailwind tokens and `cva` variants.
- [`lgtm-db`](db/index.md) (`libs/lgtm-db`): Encapsulates SeaORM migrations, private entity tables (`identities`, `reviews`, `sessions`, `comments`), and typed repositories (`IdentitiesRepository`, `ReviewsRepository`, `SessionsRepository`, `CommentsRepository`) over SQLite or PostgreSQL.
- [`lgtm-config`](config/index.md) (`libs/lgtm-config`): Merges built-in language presets with `~/.lgtm/config.toml` and `<workspace>/.lgtm/config.toml`, detects workspace languages and ancestor project roots, and initializes `<workspace>/.lgtm/` and `<git-dir>/info/exclude`.
- [`lgtm-env`](env/index.md) (`libs/lgtm-env`): Detects or installs `mise` into `<lgtm_home>/bin/mise`, synthesizes `<workspace>/.lgtm/mise.toml` (`GeneratedMiseFile`), and executes isolated `mise run`, `mise install`, and `mise exec` commands (`MiseService`).
- [`lgtm-lsp`](lsp/index.md) (`libs/lgtm-lsp`): Pools per-`(workspace_root, language_id, project_root)` stdio JSON-RPC 2.0 language server sessions (`LgtmLspManager`, `LspClientHandle`), caches open UTF-8 documents and diagnostics (`LspCacheService`), and translates CodeMirror UTF-16 offsets to and from LSP `(line, character)` coordinates.

### Data and Control Flow

#### 1. CLI and Forge Review Lifecycle (`lgtm-cli` -> `lgtm-forge` -> `lgtm-db`)

```mermaid
sequenceDiagram
    actor Caller as User / AI Agent
    participant CLI as lgtm-cli
    participant Forge as lgtm-forge (ReviewService)
    participant VCS as SnapshotStore (--local) / git + RemoteForge (PR)
    participant DB as lgtm-db (Database / Transaction)

    Caller->>CLI: lgtm review new / refresh [--local]
    CLI->>Forge: CreateReview / RefreshReview (Connect RPC)
    Forge->>VCS: Snapshot worktree (gitoxide) or fetch GitHub PR bundle (octocrab)
    opt head_sha advanced mid-flight
        Forge->>VCS: Read old_head_sha and new_head_sha blobs
        Forge->>Forge: CommentAnchor::recompute_shifts
    end
    Forge->>DB: Transaction: upsert review, sessions, comments & shifted anchors
    Forge-->>CLI: ReviewSummary
    Caller->>CLI: lgtm comment new "Check invariant" --file src/lib.rs --line 24 [--as agent]
    CLI->>Forge: CreateComment
    Forge->>VCS: Validate anchor line against head_sha blob
    Forge->>DB: Transaction: get_or_create_draft_session + create_comment
    opt ForgeBacked review
        Forge->>VCS: Sync draft comment to GitHub pending review (with AI disclosure if Agent)
        Forge->>DB: Record remote forge_id
    end
    Forge-->>CLI: CommentDetail
    Caller->>CLI: lgtm review show [--unresolved]
    CLI->>Forge: ShowReview
    Forge->>DB: Load review, sessions, and comments
    Forge->>VCS: Read base_sha and head_sha blobs -> DiffHunk::from_blobs
    Forge-->>CLI: ShowReviewResponse (rendered as terminal diff or --json)
```

#### 2. Desktop Workspace, Environment, and LSP Lifecycle (`lgtm-web` <-> `lgtm-desktop` -> `lgtm-lsp` -> `lgtm-env` -> `lgtm-config`)

```mermaid
sequenceDiagram
    participant Web as lgtm-web (Svelte 5 Services & CodeEditor)
    participant Host as lgtm-desktop (WorkspaceService & LspService)
    participant Lsp as lgtm-lsp (LgtmLspManager & LspCacheService)
    participant Env as lgtm-env (MiseService)
    participant Cfg as lgtm-config (ConfigService)
    participant Server as Language Server (stdio)

    Web->>Host: invoke("workspace_open", { path })
    Host->>Lsp: switch_workspace(prev_root, next_root)
    Lsp->>Cfg: get_config(next_root) + detect_workspace_languages
    Lsp->>Env: sync_workspace_mise(next_root) (.lgtm/mise.toml + .git/info/exclude)
    Host-->>Web: WorkspaceListing { root, entries }
    par Background language server warmup
        Lsp->>Env: build_exec_command / install_tools (ensures ~/.lgtm/bin/mise)
        Lsp->>Server: spawn + initialize / initialized
        Lsp-->>Host: LspEvent::Status(Starting -> Ready | Unavailable)
        Host-->>Web: emit("lsp-status")
    end
    Web->>Host: invoke("workspace_read", { relativePath })
    Host->>Lsp: open_or_sync_file(root, path, content)
    Lsp->>Server: textDocument/didOpen
    Host-->>Web: FileContent { content, revision, languageId, lspStatus }
    Server-->>Lsp: textDocument/publishDiagnostics
    Lsp-->>Host: LspEvent::Diagnostics (UTF-16 offsets) + LspEvent::Status
    Host-->>Web: emit("lsp-diagnostics") + emit("lsp-status")
    Web->>Host: invoke("lsp_did_change") -> 150ms debounce -> invoke("workspace_write")
    Host->>Lsp: did_change -> did_save
```

## Implementation

### Project structure

```text
lgtm/
├── apps/
│   ├── lgtm-cli/       CLI binary, daemon manager, Connect RPC client, and `lgtm run` proxy
│   ├── lgtm-desktop/   Tauri v2 desktop host, WorkspaceService, LspService, and E2E stdio IPC bridge
│   ├── lgtm-forge/     Connect RPC review server, SnapshotStore, RemoteForge, and Protobuf schemas
│   └── lgtm-web/       Svelte 5 desktop/web IDE shell, reactive services, and Tauri/RPC clients
├── libs/
│   ├── lgtm-config/    Global/workspace JSON config, JSON Schema, and language discovery SDK
│   ├── lgtm-db/        SeaORM SQLite/PostgreSQL persistence layer and named repositories
│   ├── lgtm-env/       Managed `mise` binary lifecycle and `<workspace>/.lgtm/mise.toml` synthesis SDK
│   ├── lgtm-lsp/       Stdio JSON-RPC 2.0 language server pool, document cache, and UTF-16 offset math
│   ├── lgtm-project/   Project primary checkout resolution and managed/external Git worktree lifecycle
│   ├── lgtm-rsutils/   Stateless Git path resolution, worktree CLI, slug hashing, atomic write, and URL helpers
│   ├── lgtm-session/   Desktop session persistence (`~/.lgtm/sessions/<id>.json`) and per-workspace tab retention
│   └── lgtm-ui/        Portable Helical UI + Ark UI + CodeMirror 6 Svelte 5 component library
└── e2e/
    ├── common/         Shared test harness and offline GitHub REST/GraphQL cassette replay
    ├── forge/          Vitest Connect RPC and ReviewService E2E suite
    ├── cli/            Vitest CLI local/PR review, daemon, and `lgtm run` E2E suite
    └── desktop/        Playwright Chromium and WebKit desktop workspace and LSP E2E suite
```

### Workspace and runtime storage

| Path | Owner | Purpose |
| --- | --- | --- |
| `~/.lgtm/config.toml` | `lgtm-config` | Optional user-wide `[lsp]`, `[mise]`, and `[languages.<id>]` overrides (`$LGTM_HOME/config.toml`). |
| `~/.lgtm/bin/mise` | `lgtm-env` | Managed `mise` binary installed automatically when no system `mise` is available on `PATH`. |
| `~/.lgtm/lgtm.db` | `lgtm-db` / `lgtm-forge` | Default local SQLite database storing identities, reviews, sessions, and comments. |
| `~/.lgtm/snapshots/$repoSlug` | `lgtm-forge` (`SnapshotStore`) | Bare Git repository and per-branch shadow indices (`index.<branch>`, `refs/lgtm/snapshots/<branch>`) for `--local` reviews. |
| `~/.lgtm/workspace` | `lgtm-desktop` (`WorkspaceService`) | Default desktop workspace directory created on first launch when no folder is specified. |
| `<workspace>/.lgtm/config.toml` | `lgtm-config` | Optional workspace-specific configuration overlay. |
| `<workspace>/.lgtm/mise.toml` | `lgtm-env` (`MiseService`) | Synthesized Mise tools and tasks file (`dir = "{{cwd}}"`) scoped away from any user-owned root `mise.toml`. |
| `<workspace>/.lgtm/cache/discovery.json` | `lgtm-lsp` (`LspCacheService`) | Cached workspace language and project-root discovery snapshot. |
| `<git-dir>/info/exclude` | `lgtm-config` (`ConfigService`) | Local Git exclude file updated idempotently with `.lgtm/` so LGTM metadata remains untracked without touching `.gitignore`. |

### Development flow

```mermaid
flowchart TD
    DEV_WEB["premise run dev"] --> WATCH["Rust lgtm-forge watcher (127.0.0.1:8080)"]
    DEV_WEB --> VITE["Vite dev server (127.0.0.1:5173)"]
    VITE -->|"Proxy /rpc"| WATCH
    DEV_DESK["premise run dev:desktop"] --> TAURI["cargo tauri dev (lgtm-desktop + lgtm-web)"]
    TAURI --> LSP_WARMUP["WorkspaceService + LspService + lgtm-lsp"]
```

`premise run dev` runs the standalone `lgtm-forge` watcher alongside the Vite development server, while `premise run dev:desktop` launches the native Tauri v2 desktop window backed by `apps/lgtm-desktop` and `apps/lgtm-web`. All formatting, linting, unit/integration testing, and E2E verification run through root Mise tasks invoked via Premise (`premise run format`, `premise run lint`, `premise run test`, `premise run e2e:smoke`).

### Packaging and delivery

```mermaid
flowchart TD
    FORGE_SRC["lgtm-forge + lgtm-db + lgtm-config + lgtm-env"] --> CLI_BIN["lgtm-cli (Cargo path dependencies)"]
    CLI_BIN --> ARCHIVE["Premise Linux, macOS, and Windows CLI binary archives"]
    DESK_SRC["lgtm-desktop + lgtm-web + lgtm-lsp + lgtm-env + lgtm-config"] --> TAURI_BUNDLE["Tauri native desktop bundles (.dmg / .deb / .AppImage / .exe)"]
```

All workspace Cargo crates set `publish = false` and link directly via Cargo `path` dependencies. GitHub Releases distribute precompiled `lgtm` CLI archives and native `lgtm-desktop` installers across Linux, macOS, and Windows. See [CI, Release, and Publishing Architecture](../ci.md).

## References

- [Neovim user guide](../user-guide.md).
- [Forge architecture (`lgtm-forge`)](forge/index.md).
- [Review RPC contracts](forge/reviews.md).
- [Snapshot store architecture](forge/snapshot-store.md).
- [Authorship and agent identity architecture](forge/authorship.md).
- [Database library architecture (`lgtm-db`)](db/index.md).
- [CLI architecture (`lgtm-cli`)](cli/index.md).
- [Desktop application architecture (`lgtm-desktop` & `lgtm-web`)](desktop/index.md).
- [Desktop event handling & service orchestration](desktop/events.md).
- [Desktop project and session management](desktop/projects.md).
- [Web frontend architecture](web/index.md).
- [ProjectsPane UI component](web/projects-pane.md).
- [Session library architecture (`lgtm-session`)](session/index.md).
- [Project library architecture (`lgtm-project`)](project/index.md).
- [Rust utilities library architecture (`lgtm-rsutils`)](rsutils/index.md).
- [Configuration library architecture (`lgtm-config`)](config/index.md).
- [Environment library architecture (`lgtm-env`)](env/index.md).
- [LSP library architecture (`lgtm-lsp`)](lsp/index.md).
- [End-to-end testing architecture (`e2e/`)](e2e/index.md).
- [Environment variables reference](../environment.md).
- [Development guide](../development.md).
- [ADR 0001: Initial Architecture](../../adr/0001-initial-architecture/ADR.md).
- [ADR 0002: Core Concepts and CLI Design](../../adr/0002-cli-design/ADR.md).
- [Product requirements](../../prd/0001-core-concepts/README.md).
