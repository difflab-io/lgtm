# LSP library — software architecture

## Overview

`lgtm-lsp` is the Rust Language Server Protocol client and process orchestration SDK for `lgtm-desktop`. Consuming `lgtm-config` and `lgtm-env`, it spawns and pools per-`(workspace_root, language_id, project_root)` stdio JSON-RPC 2.0 language server sessions (via `MiseService::build_exec_command` with direct binary fallback), maintains open UTF-8 document snapshots, converts between CodeMirror UTF-16 character offsets (`u32`) and LSP `(line, character)` coordinates in Rust, and provides diagnostics, hover documentation, go-to-definition locations, and plain-text diagnostics buffers.

## Requirement

### Functional

- Automatic language server pooling: Start and reuse one stdio JSON-RPC 2.0 server session per `(workspace_root, language_id, project_root)` when matching files are opened.
- Document lifecycle synchronization: Send `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didSave`, and `textDocument/didClose` notifications as files are opened, edited, saved, renamed, or deleted.
- UTF-16 offset coordinate translation: Convert between flat 0-based UTF-16 character offsets used by CodeMirror 6 and 0-based `(line, character)` positions required by LSP 3.17 entirely in Rust.
- Diagnostics streaming and text buffers: Parse `textDocument/publishDiagnostics` notifications, emit workspace-relative `LspEvent::Diagnostics` and `LspEvent::Status` events, and format full plain-text diagnostics buffers (`LspDiagnosticsBuffer`) for viewing in read-only editor tabs.
- Hover and go-to-definition: Resolve `textDocument/hover` (`LspHoverResult`) and `textDocument/definition` (`Vec<LspLocation>`) from a file path and UTF-16 cursor offset.

### Non-Functional

- Graceful degradation: If a language server binary or `mise` toolchain is missing or fails to initialize, transition the server status to `unavailable` with a descriptive message without blocking file viewing or editing.
- Clean process lifecycle: Terminate child language server processes via `shutdown`/`exit` and `SIGKILL` fallback when workspaces close or the desktop app exits.

## Design

### Usage

```rust
use std::path::Path;
use lgtm_config::ConfigService;
use lgtm_lsp::{LgtmLspManager, LspEvent};

let config_service = ConfigService::new();
let lsp_manager = LgtmLspManager::new(config_service, |event| match event {
    LspEvent::Status(status) => println!("LSP {}: {:?}", status.language_id, status.status),
    LspEvent::Diagnostics(diags) => println!("{} diagnostics for {}", diags.diagnostics.len(), diags.path),
});

let workspace = Path::new("/path/to/workspace");
let file = workspace.join("src/lib.rs");
let source = "pub fn helper() -> i32 { 42 }\n";

let (language_id, status) = lsp_manager.open_or_sync_file(workspace, &file, source);
let hover = lsp_manager.get_hover(workspace, &file, 7)?;
let definitions = lsp_manager.get_definition(workspace, &file, 7)?;
let buffer = lsp_manager.get_diagnostics_buffer(workspace, language_id.as_deref())?;
lsp_manager.shutdown_all();
```

### Components and Responsibilities

```mermaid
flowchart TB
    DESKTOP["lgtm-desktop (services/lsp.rs LspService)"]
    subgraph Library["lgtm-lsp"]
        MGR["LgtmLspManager"]
        CACHE["LspCacheService"]
        HANDLE["LspClientHandle (stdio JSON-RPC 2.0)"]
        MODELS["LspDiagnostic, LspHoverResult, LspLocation, LspDiagnosticsBuffer"]
        MGR --> CACHE
        MGR --> HANDLE
        MGR --> MODELS
    end
    CFG["lgtm-config (ConfigService) & lgtm-env (MiseService)"]
    DESKTOP --> MGR
    MGR --> CFG
    HANDLE --> PROC["mise exec -- <language-server> (or direct binary)"]
```

- `LgtmLspManager`: Owns active `LspClientHandle` sessions, coordinates with `LspCacheService` for open document snapshots and diagnostics, and exposes synchronous methods and client handles to callers such as `lgtm-desktop`'s `LspService`.
- `LspCacheService`: Manages cached project-root/mise discovery, open UTF-8 document snapshots (`didOpen`/`didChange` diffing), per-file diagnostics, and server status snapshots.
- `LspClientHandle`: Handle to an individual stdio JSON-RPC 2.0 language server session (`LspServerSession`), framing requests and notifications with `Content-Length` headers and demultiplexing responses and `textDocument/publishDiagnostics` notifications on a reader thread.
- `models::lsp` and `models::cache`: Define serializable IPC payloads (`LspServerStatusEvent`, `LspDiagnostic`, `LspFileDiagnosticsEvent`, `LspHoverResult`, `LspLocation`, `LspWorkspaceSnapshot`, `LspDiagnosticsBuffer`), cache state types (`LspSessionKey`, `LspCachedDocument`, `LspDiscoveryCache`, `LspDocumentSyncAction`), UTF-16 offset conversion functions, `file://` URI helpers, and plain-text diagnostics buffer formatting.

### Data and Control Flow

```mermaid
sequenceDiagram
    participant Web as lgtm-web (CodeEditor / LspStatusMenu)
    participant App as lgtm-desktop (LspService)
    participant Lsp as LgtmLspManager
    participant Proc as Language Server (stdio)

    Web->>App: workspace_read("src/lib.rs")
    App->>Lsp: open_or_sync_file(root, "src/lib.rs", content)
    opt Session not yet started
        Lsp->>Proc: spawn + initialize / initialized
        Lsp-->>App: LspEvent::Status(Ready)
        App-->>Web: emit("lsp-status")
    end
    Lsp->>Proc: textDocument/didOpen
    Proc-->>Lsp: textDocument/publishDiagnostics
    Note over Lsp: Convert (line, character) ranges to UTF-16 offsets
    Lsp-->>App: LspEvent::Diagnostics + LspEvent::Status
    App-->>Web: emit("lsp-diagnostics") + emit("lsp-status")
    Web->>App: lsp_hover("src/lib.rs", offset)
    App->>Lsp: get_hover(root, "src/lib.rs", offset)
    Lsp->>Proc: textDocument/hover (line, character)
    Proc-->>Lsp: Hover response
    Lsp-->>Web: LspHoverResult { contents, from, to }
```

### Interfaces and Contracts

| Method / Function                         | Signature                                                                                                                               | Description                                                                                                                                   |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `LgtmLspManager::new`                     | `fn new<F>(config_service: ConfigService, event_sink: F) -> Self`                                                                       | Create an `LgtmLspManager` forwarding `LspEvent::Status` and `LspEvent::Diagnostics` to `event_sink`.                                         |
| `LgtmLspManager::noop`                    | `fn noop(config_service: ConfigService) -> Self`                                                                                        | Create a disabled `LgtmLspManager` that performs no-ops and spawns no child language server processes.                                        |
| `LgtmLspManager::get_client_handle`       | `fn get_client_handle(&self, workspace_root: &Path, file_path: &Path) -> Result<Option<LspClientHandle>>`                               | Return an `LspClientHandle` for the language server governing `file_path`.                                                                    |
| `LgtmLspManager::open_or_sync_file`       | `fn open_or_sync_file(&self, workspace_root: &Path, file_path: &Path, content: &str) -> (Option<String>, Option<LspServerStatusEvent>)` | Detect file language, start or reuse the project's language server, and send `didOpen` or `didChange`.                                        |
| `LgtmLspManager::did_change`              | `fn did_change(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()>`                                            | Update the in-memory document snapshot and send `textDocument/didChange`.                                                                     |
| `LgtmLspManager::did_save`                | `fn did_save(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()>`                                              | Synchronize latest content and send `textDocument/didSave`.                                                                                   |
| `LgtmLspManager::did_close`               | `fn did_close(&self, workspace_root: &Path, file_path: &Path) -> Result<()>`                                                            | Remove the open document snapshot and send `textDocument/didClose`.                                                                           |
| `LgtmLspManager::get_hover`               | `fn get_hover(&self, workspace_root: &Path, file_path: &Path, offset: u32) -> Result<Option<LspHoverResult>>`                           | Convert UTF-16 `offset` to `(line, character)` and query `textDocument/hover`.                                                                |
| `LgtmLspManager::get_definition`          | `fn get_definition(&self, workspace_root: &Path, file_path: &Path, offset: u32) -> Result<Vec<LspLocation>>`                            | Convert UTF-16 `offset` to `(line, character)`, query `textDocument/definition`, and return workspace-relative locations with UTF-16 offsets. |
| `LgtmLspManager::get_workspace_snapshot`  | `fn get_workspace_snapshot(&self, workspace_root: &Path) -> Result<LspWorkspaceSnapshot>`                                               | Return all active language server statuses and diagnostics for `workspace_root`.                                                              |
| `LgtmLspManager::get_diagnostics_buffer`  | `fn get_diagnostics_buffer(&self, workspace_root: &Path, language_id: Option<&str>) -> Result<LspDiagnosticsBuffer>`                    | Render a plain-text diagnostics report for all active servers or a single language server.                                                    |
| `LgtmLspManager::restart_language_server` | `fn restart_language_server(&self, workspace_root: &Path, language_id: &str) -> Result<LspServerStatusEvent>`                           | Restart all active sessions for `language_id` in `workspace_root` and re-synchronize open documents.                                          |

## Implementation

### Package layout

```text
libs/lgtm-lsp/
├── Cargo.toml
├── mise.toml
├── src/
│   ├── lib.rs                 Public LgtmLspManager, LspClientHandle, LspCacheService, and model exports
│   ├── error.rs               LspError and Result alias
│   ├── models/
│   │   ├── mod.rs
│   │   ├── cache.rs           LspSessionKey, LspCachedDocument, LspDiscoveryCache, LspDocumentSyncAction
│   │   └── lsp.rs             LSP domain models, UTF-16 offset math, URI helpers, and buffer renderer
│   └── services/
│       ├── mod.rs
│       ├── cache.rs           LspCacheService
│       └── manager.rs         LgtmLspManager and LspClientHandle stdio JSON-RPC 2.0 client
└── tests/
    ├── common/mod.rs          Shared mock stdio JSON-RPC 2.0 language server fixture
    ├── cache.rs               LspCacheService BDD integration suite
    └── manager.rs             LgtmLspManager BDD integration suite
```

### Mise execution with direct binary fallback

When spawning a language server whose `LanguageConfig` declares `mise_tools`, `LgtmLspManager` first attempts `mise exec --cd <project_root> <tool@version>... -- <command> <args...>` with `MISE_CONFIG_FILE=<workspace>/.lgtm/mise.toml`. If `mise` or the managed tool is not installed in the current environment, `LgtmLspManager` falls back to spawning `<command> <args...>` directly from `PATH` before reporting `LspServerStatus::Unavailable`.

## References

- [Configuration library (`lgtm-config`) architecture](../config/index.md).
- [Environment library (`lgtm-env`) architecture](../env/index.md).
- [Desktop application (`lgtm-desktop`) architecture](../desktop/index.md).
- [Application architecture](../index.md).
