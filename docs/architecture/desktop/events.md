# Desktop event handling & service orchestration — software architecture

## Overview

`apps/lgtm-desktop/src/events.rs` provides an in-process backend event bus (`AppEvents` and `AppEvent`) wired in `apps/lgtm-desktop/src/app.rs`. It decouples backend domain services (`WorkspaceService`, `ConfigService`, `LspService`) to eliminate circular imports, runs synchronous state transitions before notifying the UI, and offloads slow work onto background threads without blocking callers.

## Requirement

### Functional

- Typed backend event bus (`events.rs`): Dispatch all backend domain and lifecycle events as strongly typed `AppEvent` variants through `AppEvents`.
- Centralized backend orchestration (`app.rs`): Register the `AppEvents` handler in `app.rs` to invoke `LspService` and `ConfigService` methods and push UI updates over Tauri IPC only after synchronous backend state is consistent.
- Dual frontend update path: Update the frontend via direct IPC command return values for request/response operations, and via `app_handle.emit` (plus `BridgeEventForwarder` in E2E) for background thread, OS menu/window, or cross-domain side-effect notifications.

### Non-Functional

- Strong typing & auditable patterns: Define all events in `AppEvent` (`events.rs`) and handle them in a single `match` in `app.rs` so dispatch and reaction sites are immediately identifiable during code review.
- No circular dependencies: `WorkspaceService`, `ConfigService`, and `LspService` never import each other or `tauri::AppHandle`.
- Non-blocking concurrency: Background LSP and toolchain tasks run outside service locks and emit follow-up `AppEvent::LspStatus` and `AppEvent::LspDiagnostics` events when complete.

## Design

### Usage

#### Dispatching and Handling Events (`apps/lgtm-desktop/src/app.rs` & `events.rs`)

```rust
// Dispatching a backend domain event
state.events.dispatch(AppEvent::FileSaved {
    root,
    path,
    content,
    is_config,
});

// Central handler in app.rs
match event {
    AppEvent::WorkspaceOpened { previous_root, next_root } => {
        lsp.switch_workspace(previous_root.as_deref(), &next_root);
    }
    AppEvent::FileOpened(payload) => events.emit_to_webview("workspace-open-file", &payload),
    AppEvent::FileSaved { root, path, content, is_config } => {
        let _ = lsp.did_save(&root, &path, &content);
        if is_config && let Ok(snapshot) = config.get_config_snapshot(Some(&root)) {
            lsp.reload_workspace_config(&root);
            events.emit_to_webview("config-updated", &snapshot);
        }
    }
    AppEvent::FileClosed { root, path } => {
        let _ = lsp.did_close(&root, &path);
    }
    AppEvent::LspStatus(payload) => events.emit_to_webview("lsp-status", &payload),
    AppEvent::LspDiagnostics(payload) => events.emit_to_webview("lsp-diagnostics", &payload),
    AppEvent::QuitRequested => events.emit_to_webview("workspace-quit-requested", &()),
}
```

### Components and Responsibilities

#### 1. Backend Class Structure (`lgtm-desktop`)

```mermaid
classDiagram
    direction TB

    class AppState {
        +Arc~Mutex~WorkspaceService~~ workspace
        +ConfigService config
        +Arc~LspService~ lsp
        +AppEvents events
        +Arc~AtomicBool~ frontend_ready
    }

    class AppEvents {
        +new(webview_sink, handler) AppEvents
        +dispatch(event: AppEvent) void
        +emit_to_webview(name: str, payload: Serialize) void
    }

    class AppEvent {
        <<enumeration>>
        WorkspaceOpened
        FileOpened(FileOpenedEvent)
        FileSaved
        FileClosed
        LspStatus(LspServerStatusEvent)
        LspDiagnostics(LspFileDiagnosticsEvent)
        QuitRequested
    }

    class WorkspaceService {
        +open(path) Result~WorkspaceListing~
        +get_file_content(path) Result~FileContent~
        +write_file(path, content) Result~FileContent~
        +rename(path, name) Result~WorkspaceNode~
        +delete(path) Result~()~
        +is_config_path(path) bool
    }

    class LspService {
        +switch_workspace(prev, next) void
        +reload_workspace_config(root) void
        +open_or_sync_file(root, path, content)
        +did_change(root, path, content) Result~()~
        +did_save(root, path, content) Result~()~
        +did_close(root, path) Result~()~
        +shutdown_all() void
    }

    AppState *-- WorkspaceService
    AppState *-- LspService
    AppState *-- AppEvents
    AppEvents --> AppEvent : dispatches
```

#### 2. Event Propagation Across Lanes

```mermaid
flowchart TD
    subgraph Origin["1. Origin"]
        O_OPEN["workspace_open"]
        O_SAVE["workspace_write"]
        O_CLOSE["workspace_rename / delete"]
        O_LSP["LgtmLspManager Thread"]
        O_OS["OS Menu / ExitRequested"]
    end

    subgraph Bus["2. Backend Event (events.rs)"]
        E_OPEN["AppEvent::WorkspaceOpened"]
        E_SAVE["AppEvent::FileSaved"]
        E_CLOSE["AppEvent::FileClosed"]
        E_LSP["AppEvent::LspStatus / LspDiagnostics"]
        E_OS["AppEvent::FileOpened / QuitRequested"]
    end

    subgraph Backend["3. Backend Reaction (app.rs)"]
        B_OPEN["LspService::switch_workspace<br/>(sync cache + async warmup)"]
        B_SAVE["LspService::did_save<br/>+ if is_config: ConfigService::get_config_snapshot<br/>+ LspService::reload_workspace_config"]
        B_CLOSE["LspService::did_close"]
        B_PASS["Forward to Webview Sink"]
    end

    subgraph Frontend["4. Frontend Update (lgtm-web)"]
        F_RET["IPC Command Return<br/>(WorkspaceListing / FileContent)"]
        F_CFG["emit 'config-updated'<br/>-> SettingsService -> ThemeService"]
        F_LSP["emit 'lsp-status' / 'lsp-diagnostics'<br/>-> LspService"]
        F_OS["emit 'workspace-open-file' / 'workspace-quit-requested'<br/>-> WorkspaceService"]
    end

    O_OPEN --> E_OPEN --> B_OPEN --> F_RET
    B_OPEN -.->|"warmup threads"| E_LSP
    O_SAVE --> E_SAVE --> B_SAVE --> F_RET
    B_SAVE -->|"is_config = true"| F_CFG
    O_CLOSE --> E_CLOSE --> B_CLOSE --> F_RET
    O_LSP --> E_LSP --> B_PASS --> F_LSP
    O_OS --> E_OS --> B_PASS --> F_OS
```

### Data and Control Flow

#### 1. Backend Event Dispatch (`WorkspaceOpened` & `FileSaved`)

```mermaid
sequenceDiagram
    participant Cmd as app.rs IPC Command
    participant Ws as WorkspaceService
    participant Bus as AppEvents (events.rs)
    participant Lsp as LspService
    participant Cfg as ConfigService

    Cmd->>Ws: write_file(relative_path, content)
    Ws-->>Cmd: (FileContent, root, path, is_config = true)
    Cmd->>Bus: dispatch(AppEvent::FileSaved)
    Bus->>Lsp: 1. did_save(root, path, content)
    Bus->>Cfg: 2. get_config_snapshot(Some(root))
    Cfg-->>Bus: ResolvedConfigSnapshot
    Bus->>Lsp: 3. reload_workspace_config(root)
    Bus->>Bus: 4. emit_to_webview("config-updated", snapshot)
    Cmd-->>Cmd: Return FileContent to IPC caller
```

#### 2. Background LSP & OS Lifecycle Push to Frontend

```mermaid
sequenceDiagram
    participant Bg as LgtmLspManager / OS Hook
    participant Bus as AppEvents (events.rs)
    participant Web as Shell.svelte (lgtm-web)
    participant Svc as Frontend Service

    Bg->>Bus: dispatch(AppEvent::LspDiagnostics(event))
    Bus->>Web: emit_to_webview("lsp-diagnostics", event)
    Web->>Svc: handleDiagnostics(event)

    Bg->>Bus: dispatch(AppEvent::QuitRequested)
    Bus->>Web: emit_to_webview("workspace-quit-requested", ())
    Web->>Svc: await flushSave()
    Web->>Bus: invoke("workspace_finish_quit")
```

### Interfaces and Contracts

#### 1. Backend Event Shapes (`apps/lgtm-desktop/src/events.rs`)

```rust
pub enum AppEvent {
    WorkspaceOpened {
        previous_root: Option<PathBuf>,
        next_root: PathBuf,
    },
    FileOpened(FileOpenedEvent),
    FileSaved {
        root: PathBuf,
        path: PathBuf,
        content: String,
        is_config: bool,
    },
    FileClosed {
        root: PathBuf,
        path: PathBuf,
    },
    LspStatus(LspServerStatusEvent),
    LspDiagnostics(LspFileDiagnosticsEvent),
    QuitRequested,
}
```

#### 2. Webview Push Events (`AppEvents::emit_to_webview`)

| Wire Event Name | Payload Shape | Source `AppEvent` |
| --- | --- | --- |
| `"lsp-status"` | `LspServerStatusEvent` | `AppEvent::LspStatus` |
| `"lsp-diagnostics"` | `LspFileDiagnosticsEvent` | `AppEvent::LspDiagnostics` |
| `"config-updated"` | `ResolvedConfigSnapshot` | `AppEvent::FileSaved { is_config: true, .. }` |
| `"workspace-open-file"` | `FileOpenedEvent` | `AppEvent::FileOpened` |
| `"workspace-quit-requested"` | `()` | `AppEvent::QuitRequested` |

## References

- [Desktop application (`lgtm-desktop`) architecture](index.md).
- [Web frontend (`lgtm-web`) architecture](../web/index.md).
- [LSP library (`lgtm-lsp`) architecture](../lsp/index.md).
- [Configuration library (`lgtm-config`) architecture](../config/index.md).
