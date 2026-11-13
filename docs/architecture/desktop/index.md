# Desktop application (`lgtm-desktop` & `lgtm-web`) — software architecture

## Overview

`lgtm-desktop` is the native Tauri v2 desktop application shell for LGTM, paired with the reactive Svelte 5 IDE shell in `lgtm-web` and reusable UI primitives in `lgtm-ui`. It provides local workspace browsing and file editing, coordinates workspace lifecycle transitions with `lgtm-lsp`, `lgtm-env`, and `lgtm-config`, streams asynchronous LSP server statuses and file diagnostics to the frontend over Tauri events, and guarantees clean language server process shutdown and unsaved-buffer flushing on workspace switch or application exit.

## Requirement

### Functional

- Open the default workspace (`~/.lgtm/workspace`) or any selected folder, list files and directories, and create, read, save, rename, or move entries to the OS Trash.
- Provide language server integration for any workspace opened in the desktop application.
- Synchronize open files, unsaved editor changes, file saves, renames, and deletions with active language servers.
- Stream live language server statuses and file diagnostics to the UI, and provide hover documentation, go-to-definition, read-only diagnostics buffers, and server restarts.
- Flush pending auto-saves and stop child language server processes cleanly before closing the window or switching workspaces.

### Non-Functional

- Manage background services and child processes efficiently so the desktop application stays low-latency and nimble.

## Design

### Components and Responsibilities

```mermaid
flowchart TB
    subgraph Frontend["lgtm-web (Svelte 5) & lgtm-ui"]
        SHELL["Shell.svelte / ContentPane.svelte / LspStatusMenu.svelte"]
        WS_VM["WorkspaceService (workspaceService.svelte.ts)"]
        LSP_VM["LspService (lspService.svelte.ts)"]
        CLIENT["TauriWorkspaceClient (workspaceClient.ts)"]
        SHELL --> WS_VM
        WS_VM --> LSP_VM
        WS_VM --> CLIENT
        LSP_VM --> CLIENT
    end

    subgraph Desktop["lgtm-desktop (Tauri v2 Rust Host)"]
        IPC["app.rs (Tauri Commands & RunEvent Lifecycle)"]
        WS_SVC["services/workspace.rs (WorkspaceService)"]
        DESK_LSP["services/lsp.rs (Desktop LspService)"]
        IPC --> WS_SVC
        IPC --> DESK_LSP
        WS_SVC --> DESK_LSP
    end

    subgraph Libraries["Rust Workspace SDKs"]
        LSP_MGR["lgtm-lsp (LgtmLspManager & LspCacheService)"]
        ENV_SVC["lgtm-env (MiseService)"]
        CFG_SVC["lgtm-config (ConfigService)"]
        DESK_LSP --> LSP_MGR
        LSP_MGR --> ENV_SVC
        LSP_MGR --> CFG_SVC
        ENV_SVC --> CFG_SVC
    end

    CLIENT <-->|"Tauri invoke + lsp-status / lsp-diagnostics events"| IPC
    ENV_SVC --> MISE["<lgtm_home>/bin/mise & <workspace>/.lgtm/mise.toml"]
    LSP_MGR --> SERVERS["Language Server Processes (stdio JSON-RPC 2.0)"]
```

- `app.rs` (`lgtm-desktop`): Registers a unified `AppState` (`workspace`, `config`, `lsp`, `session`, `project`, `frontend_ready`, `events`) in Tauri managed state, dispatches `workspace_*`, `lsp_*`, `config_get`, `config_update`, `session_*`, and `project_*` IPC commands onto `spawn_blocking`, emits `config-updated` when `workspace_write` or `config_update` saves a configuration file, and coordinates the `RunEvent::ExitRequested` quit handshake.
- `services::workspace::WorkspaceService` (`lgtm-desktop`): Owns the active `Workspace` root, validates workspace-relative paths (plus `~/.lgtm/config.json` and `.lgtm/config.json`), reads global and local configuration files via `get_file_content`, and performs atomic file/folder CRUD.
- `services::lsp::LspService` (`lgtm-desktop`): Bridges `lgtm-lsp`'s `LgtmLspManager` to the Tauri `AppHandle` (and optional E2E stdio bridge forwarder), translating `LspEvent::Status` and `LspEvent::Diagnostics` callbacks into `app_handle.emit("lsp-status", ...)` and `app_handle.emit("lsp-diagnostics", ...)`.
- `LgtmLspManager` & `LspCacheService` (`lgtm-lsp`): Manage per-`(workspace_root, language_id, project_root)` stdio JSON-RPC 2.0 sessions, cache open UTF-8 document snapshots and diagnostics, and coordinate with `MiseService` (`lgtm-env`) and `ConfigService` (`lgtm-config`) to launch language servers.
- `WorkspaceService`, `ProjectService`, `SessionService`, `LspService`, `SettingsService`, & `HotkeyService` (`lgtm-web`): Reactive Svelte 5 `$state` services that manage open editor tabs, 150ms debounced auto-save (`flushSave`), live `lsp_did_change` notifications, active language server indicators, read-only diagnostics buffer tabs, modal keybindings (`WhichKeyProvider`, `CommandMenu`, `CommandPalette`), and live `config-updated` preference synchronization. See [Web frontend architecture](../web/index.md) and [Modal keybindings architecture](../web/keybindings.md).

### Data and Control Flow

#### 1. Startup, Workspace Switch, and Warmup Lifecycle

```mermaid
sequenceDiagram
    participant UI as lgtm-web (Shell / WorkspaceService)
    participant Tauri as lgtm-desktop (app.rs & WorkspaceService)
    participant DeskLsp as lgtm-desktop (LspService)
    participant LspMgr as lgtm-lsp (LgtmLspManager)
    participant Env as lgtm-env (MiseService)
    participant Cfg as lgtm-config (ConfigService)

    UI->>Tauri: invoke("workspace_frontend_ready")
    UI->>Tauri: invoke("workspace_open", { path })
    Tauri->>DeskLsp: switch_workspace(previous_root, next_root)
    opt previous_root != next_root
        DeskLsp->>LspMgr: shutdown_workspace(previous_root)
    end
    DeskLsp->>LspMgr: prepare_workspace(next_root)
    LspMgr->>Cfg: get_config(next_root) + detect_workspace_languages
    LspMgr->>Env: sync_workspace_mise(next_root)
    Env->>Cfg: ensure_workspace_dir(next_root) (.lgtm/ + .git/info/exclude)
    Env-->>Env: write <workspace>/.lgtm/mise.toml
    loop Each detected enabled language (background thread)
        LspMgr-->>DeskLsp: LspEvent::Status(Starting)
        DeskLsp-->>UI: emit("lsp-status", Starting)
        LspMgr->>Env: build_exec_command / install_tools (ensures mise binary)
        LspMgr-->>DeskLsp: LspEvent::Status(Ready | Unavailable)
        DeskLsp-->>UI: emit("lsp-status", Ready | Unavailable)
    end
    Tauri-->>UI: WorkspaceListing { root, entries }
    UI->>Tauri: invoke("lsp_get_state")
```

#### 2. File Editing, Diagnostics, Navigation, and Quit Handshake

```mermaid
sequenceDiagram
    participant Editor as lgtm-web (CodeEditor / WorkspaceService)
    participant Tauri as lgtm-desktop (app.rs)
    participant LspMgr as lgtm-lsp (LgtmLspManager)
    participant Server as Language Server (stdio)

    Editor->>Tauri: invoke("workspace_read", { relativePath })
    Tauri->>LspMgr: open_or_sync_file(root, path, content)
    LspMgr->>Server: textDocument/didOpen
    Tauri-->>Editor: FileContent { content, revision, languageId, lspStatus }
    Server-->>LspMgr: textDocument/publishDiagnostics
    LspMgr-->>Tauri: LspEvent::Diagnostics + LspEvent::Status
    Tauri-->>Editor: emit("lsp-diagnostics") + emit("lsp-status")

    Editor->>Tauri: invoke("lsp_did_change", { relativePath, content })
    Tauri->>LspMgr: did_change(root, path, content)
    LspMgr->>Server: textDocument/didChange

    Note over Editor: 150ms debounce expires -> flushSave()
    Editor->>Tauri: invoke("workspace_write", { relativePath, content })
    Tauri->>LspMgr: did_save(root, path, content)
    LspMgr->>Server: textDocument/didSave

    Note over Tauri: User closes window (RunEvent::ExitRequested)
    Tauri-->>Editor: emit("workspace-quit-requested")
    Editor->>Editor: await flushSave()
    Editor->>Tauri: invoke("workspace_finish_quit")
    Tauri->>LspMgr: shutdown_all() (shutdown/exit + kill child servers)
    Tauri->>Tauri: app.exit(0)
```

### Interfaces and Contracts

| Tauri Command / Event | Direction | Payload / Signature | Description |
| --- | --- | --- | --- |
| `workspace_open` | UI -> Host | `(path?: string) -> WorkspaceListing` | Open default or selected workspace directory, switch LSP workspace context, and list root entries. |
| `workspace_list` | UI -> Host | `(relativePath: string) -> WorkspaceNode[]` | List sorted child directories and files inside a workspace-relative folder. |
| `workspace_read` | UI -> Host | `(relativePath: string) -> FileContent` | Read UTF-8 file content, synchronize `didOpen` with `LgtmLspManager`, and return language ID and server status. |
| `workspace_write` | UI -> Host | `(relativePath: string, content: string) -> FileContent` | Atomically write UTF-8 file content to disk and send `textDocument/didSave`. |
| `workspace_create_file` / `workspace_create_folder` | UI -> Host | `(name: string, parentPath?: string) -> WorkspaceNode` | Exclusively create a new file or folder inside the workspace. |
| `workspace_rename` | UI -> Host | `(relativePath: string, name: string) -> WorkspaceNode` | Rename a workspace entry without overwriting existing targets and close its old LSP document URI. |
| `workspace_delete` | UI -> Host | `(relativePath: string) -> void` | Move a workspace entry to OS Trash and close its LSP document URI. |
| `workspace_frontend_ready` | UI -> Host | `() -> void` | Mark the frontend quit listener ready so `ExitRequested` waits for `flushSave()`. |
| `workspace_finish_quit` | UI -> Host | `() -> void` | Terminate all active language server processes via `LspService::shutdown_all` and exit. |
| `lsp_did_change` | UI -> Host | `(relativePath: string, content: string) -> void` | Update cached buffer text and send `textDocument/didChange`. |
| `lsp_hover` | UI -> Host | `(relativePath: string, offset: number) -> LspHoverResult \| null` | Query `textDocument/hover` at a CodeMirror UTF-16 offset. |
| `lsp_goto_definition` | UI -> Host | `(relativePath: string, offset: number) -> LspLocation[]` | Query `textDocument/definition` at a CodeMirror UTF-16 offset and return workspace-relative locations. |
| `lsp_completion` | UI -> Host | `(relativePath: string, offset: number) -> LspCompletionItem[]` | Query `textDocument/completion` at a CodeMirror UTF-16 offset and return completion candidates. |
| `lsp_signature_help` | UI -> Host | `(relativePath: string, offset: number) -> LspSignatureHelp \| null` | Query `textDocument/signatureHelp` at a CodeMirror UTF-16 offset and return active function signatures. |
| `lsp_get_state` | UI -> Host | `() -> LspWorkspaceSnapshot` | Return all tracked language server statuses and file diagnostics for the active workspace. |
| `lsp_get_diagnostics_buffer` | UI -> Host | `(languageId?: string) -> LspDiagnosticsBuffer` | Render a read-only plain-text diagnostics report for all active servers or a single language. |
| `lsp_restart` | UI -> Host | `(languageId: string) -> LspServerStatusEvent` | Restart all sessions for `languageId` in the active workspace and re-open tracked documents. |
| `config_get` | UI -> Host | `() -> ResolvedConfigSnapshot` | Return effective deep-merged `LgtmConfig` plus global and project `LgtmConfigPatch` trees. |
| `config_update` | UI -> Host | `(patch: LgtmConfigPatch, scope?: ConfigScope) -> ResolvedConfigSnapshot` | Atomically apply `patch` to global or project `config.json`, reload workspace LSP config, emit `config-updated`, and return the updated snapshot. |
| `lsp-status` | Host -> UI | `LspServerStatusEvent` | Real-time language server lifecycle status and error/warning counts. |
| `lsp-diagnostics` | Host -> UI | `LspFileDiagnosticsEvent` | Real-time workspace-relative or global config file diagnostics with UTF-16 offsets. |
| `config-updated` | Host -> UI | `ResolvedConfigSnapshot` | Live resolved nested settings snapshot emitted when `workspace_write` saves `~/.lgtm/config.json` or `<workspace>/.lgtm/config.json`. |
| `workspace-open-file` | Host -> UI | `FileOpenedEvent` | Requests the frontend to open a target file path (`~/.lgtm/config.json` or `.lgtm/config.json`) in an editor tab when triggered from native menus. |
| `workspace-quit-requested` | Host -> UI | `()` | Signals the frontend to flush pending saves before calling `workspace_finish_quit`. |

## Implementation

### Package layout

```text
apps/
├── lgtm-desktop/
│   ├── Cargo.toml
│   ├── Tauri.toml
│   └── src/
│       ├── main.rs                    Desktop binary entrypoint and --ipc-bridge E2E mode
│       ├── lib.rs                     Public crate exports
│       ├── app.rs                     Tauri state registration, IPC command handlers, and quit lifecycle
│       ├── events.rs                  AppEvent and AppEvents backend event bus
│       ├── menus.rs                   Native application menu bar construction and menu event routing
│       ├── error.rs                   Serializable IPC error codes and messages
│       ├── models/
│       │   ├── file_content.rs        UTF-8 FileContent with SHA-256 revision and LSP metadata
│       │   └── workspace.rs           Workspace, WorkspaceListing, and WorkspaceNode validation
│       └── services/
│           ├── workspace.rs           WorkspaceService filesystem orchestration and LSP hooks
│           ├── lsp.rs                 Desktop LspService event emitter and workspace lifecycle adapter
│           └── ipc_bridge.rs          JSON-over-stdio bridge for headless Playwright E2E tests
└── lgtm-web/src/lib/
    ├── clients/workspaceClient.ts     WorkspaceClient interface and TauriWorkspaceClient adapter
    ├── services/
    │   ├── workspaceService.svelte.ts Reactive workspace state, tab management, and debounced save orchestration
    │   └── lspService.svelte.ts       Reactive LSP status, diagnostics, hover, and navigation service
    └── components/
        ├── Shell.svelte               Top-level IDE chrome, breadcrumbs, and LspStatusMenu mount
        ├── WorkspaceExplorer.svelte   Sidebar workspace selector, file tree, and context menu
        ├── ContentPane.svelte         TabBar and CodeEditor / Markdown / Diagnostics view host
        └── LspStatusMenu.svelte       Compact header menu displaying per-language server health and actions
```

### Headless E2E IPC Bridge

When compiled with the `e2e-bridge` feature and launched with `--ipc-bridge <home>`, `lgtm-desktop` initializes the exact same `WorkspaceService` and `LspService` graph used by the native Wry window, forwarding `lsp-status` and `lsp-diagnostics` events over stdout lines so Playwright Chromium/WebKit tests exercise the full Rust workspace, `lgtm-lsp`, `lgtm-env`, and `lgtm-config` stack deterministically.

## References

- [Desktop event handling & service orchestration](events.md).
- [Desktop project and session management](projects.md).
- [Session library (`lgtm-session`) architecture](../session/index.md).
- [Project library (`lgtm-project`) architecture](../project/index.md).
- [Rust utilities library (`lgtm-rsutils`) architecture](../rsutils/index.md).
- [ProjectsPane UI component](../web/projects-pane.md).
- [LSP library (`lgtm-lsp`) architecture](../lsp/index.md).
- [Environment library (`lgtm-env`) architecture](../env/index.md).
- [Configuration library (`lgtm-config`) architecture](../config/index.md).
- [End-to-end testing architecture](../e2e/index.md).
- [Application architecture](../index.md).
