# Desktop project and session management - software architecture

## Overview

`lgtm-desktop` coordinates sessions, projects, workspaces, and language servers for the desktop application. The session tracks open projects and per-checkout tabs, each project groups a primary checkout and its linked Git worktrees, and the workspace service edits the active checkout directory.

## Requirement

### Functional

- Restore the previous session on startup, including the active sidebar view, open projects, open worktrees, and active workspace.
- Remember each project checkout and worktree state separately, including modified tabs, the last active tab, and expanded explorer folders.
- Switch between a project primary checkout and its worktrees immediately by swapping active state on a single mounted editor and explorer view.
- Create, attach, or remove project worktrees, deleting disk storage for LGTM-managed worktrees while only detaching external worktrees from the session.
- Save pending file changes and session state before the application window closes.

### Non-Functional

- Switch workspaces with zero UI blocking by applying in-memory workspace snapshots immediately and flushing disk writes and session saves asynchronously in the background.
- Keep only the active workspace mounted in the UI and stop language servers for inactive workspaces so inactive projects and worktrees consume no background processes or DOM nodes.
- Run all project, worktree, and session operations locally without a background daemon.
- Isolate session, workspace, and language server services so an error in one service does not corrupt another.

## Design

### Components and Responsibilities

```mermaid
flowchart TB
    subgraph Web["lgtm-web (Svelte 5 Frontend)"]
        SHELL["Shell.svelte"]
        PANE["ProjectsPane.svelte & ContentPane.svelte"]
        HOTKEY["HotkeyService (hotkeyService.svelte.ts)"]
        WS_UI["WorkspaceService (workspaceService.svelte.ts)"]
        CLIENT["TauriWorkspaceClient (workspaceClient.ts)"]
        SHELL --> PANE
        SHELL --> HOTKEY
        SHELL --> WS_UI
        HOTKEY --> WS_UI
        WS_UI --> CLIENT
    end

    subgraph Host["lgtm-desktop (Tauri v2 Rust Host)"]
        APP["app.rs (AppState & IPC Commands)"]
        BUS["events.rs (AppEvents)"]
        WS_HOST["services/workspace.rs (WorkspaceService)"]
        LSP_HOST["services/lsp.rs (Desktop LspService)"]
        APP --> BUS
        APP --> WS_HOST
        APP --> LSP_HOST
    end

    subgraph Libraries["Shared Rust Libraries (libs/)"]
        SESS_LIB["lgtm-session (SessionService)"]
        PROJ_LIB["lgtm-project (ProjectService)"]
        RSUTILS["lgtm-rsutils (git, paths, url)"]
        LSP_LIB["lgtm-lsp (LgtmLspManager)"]
        CFG_LIB["lgtm-config (ConfigService)"]
        SESS_LIB --> PROJ_LIB
        PROJ_LIB --> RSUTILS
        SESS_LIB --> RSUTILS
        WS_HOST --> CFG_LIB
        LSP_HOST --> LSP_LIB
    end

    CLIENT <-->|"Tauri IPC + session-updated"| APP
    APP --> SESS_LIB
    SESS_LIB --> STORE[("~/.lgtm/sessions/default.json\n~/.lgtm/worktrees/<project>/<branch>")]
```

- `AppState` (`apps/lgtm-desktop/src/app.rs`): Holds `session` (`lgtm_session::SessionService`), `workspace` (`WorkspaceService`), `config` (`ConfigService`), `lsp` (`LspService`), and `events` (`AppEvents`).
- `AppEvents` (`apps/lgtm-desktop/src/events.rs`): Routes workspace-switch events between `SessionService`, `WorkspaceService`, and `LspService` so the services stay decoupled.
- `SessionService` (`libs/lgtm-session`): Owns `~/.lgtm/sessions/default.json`, tracks the ordered `projects`, `sidebar_widget`, and `active_workspace_root`, and stores per-workspace `PersistedTab` lists (`edited || last_opened`).
- `ProjectService` (`libs/lgtm-project`): Resolves repository root checkouts (`Project`) and linked Git worktrees (`Worktree`) using `lgtm-rsutils`.
- `WorkspaceService` (`apps/lgtm-desktop/src/services/workspace.rs`): Operates on the single active checkout directory (`active_workspace_root`), which is either a project `root_path` or a linked `Worktree.path`. It reads files, writes files, and lists directory entries.
- `WorkspaceService` & `HotkeyService` (`apps/lgtm-web`): Reactive Svelte 5 services. `WorkspaceService` manages the active checkout file tree, open editor tabs, in-memory workspace cache, and session/project state, while `HotkeyService` registers `WhichKey` leader bindings and `CommandPalette` commands.

### Data and Control Flow

#### 1. Startup Session Restore (`~/.lgtm/sessions/default.json`)

```mermaid
sequenceDiagram
    participant UI as lgtm-web (Shell / WorkspaceService)
    participant Host as lgtm-desktop (app.rs)
    participant Sess as lgtm-session (SessionService)
    participant Ws as lgtm-desktop (WorkspaceService)
    participant Lsp as lgtm-desktop (LspService)

    UI->>Host: invoke("workspace_open", {})
    Host->>Sess: get_session("default")
    Sess-->>Host: Session { sidebar_widget, active_workspace_root, projects, workspaces }
    Host->>Ws: open(active_workspace_root)
    Ws-->>Host: WorkspaceListing { root, entries }
    Host->>Lsp: switch_workspace(None, root)
    Host-->>UI: WorkspaceListing { root, entries }
    UI->>Host: invoke("session_get", { sessionId: "default" })
    Host->>Sess: get_session("default")
    Sess-->>UI: Session { sidebar_widget, active_workspace_root, projects, workspaces }
    UI->>UI: Set sidebarWidget ("explorer" | "projects") and expandedDirs
    loop Each PersistedTab in workspaces[root].tabs
        UI->>Host: invoke("workspace_read", { relativePath: tab.path })
    end
```

#### 2. Immediate Switching Between Project Primary Checkout and Linked Worktrees

```mermaid
sequenceDiagram
    participant UI as lgtm-web (WorkspaceService)
    participant Host as lgtm-desktop (app.rs)
    participant Sess as lgtm-session (SessionService)
    participant Ws as lgtm-desktop (WorkspaceService)
    participant Bus as AppEvents (events.rs)
    participant Lsp as lgtm-desktop (LspService)

    Note over UI: User clicks a project or worktree in ProjectsPane
    UI->>UI: Filter outgoing tabs (edited || lastOpened) & update in-memory workspaceCache
    alt Target workspace is warm in workspaceCache
        UI->>UI: Immediately swap single mounted Editor & Explorer to cached snapshot (0ms)
    end
    UI--)Host: Async invoke("session_save", { patch: outgoingState })
    UI--)Host: Async invoke("workspace_open", { path: targetPath })
    Host->>Ws: open(targetPath)
    Host->>Sess: open_workspace("default", targetPath)
    Host-->>UI: Reconcile fresh WorkspaceListing & Session
    Host--)Bus: Async dispatch(AppEvent::WorkspaceOpened & AppEvent::SessionUpdated)
    Bus--)Lsp: Stop previous_root LSP processes & lazily start next_root on active file
```

#### 3. Application Exit and Session Snapshot Save

```mermaid
sequenceDiagram
    participant OS as Window Close (RunEvent::ExitRequested)
    participant Host as lgtm-desktop (app.rs)
    participant UI as lgtm-web (WorkspaceService)
    participant Sess as lgtm-session (SessionService)
    participant Lsp as lgtm-desktop (LspService)

    OS->>Host: ExitRequested
    Host-->>UI: emit("workspace-quit-requested")
    UI->>UI: await workspace.flushSave()
    UI->>Host: invoke("session_save", { patch })
    Host->>Sess: save_workspace_state("default", patch)
    Note over Sess: Write ~/.lgtm/sessions/default.json with edited and last_opened tabs
    UI->>Host: invoke("workspace_finish_quit")
    Host->>Lsp: shutdown_all()
    Host->>Host: app.exit(0)
```

### Interfaces and Contracts

| Tauri Command / Event | Direction | Payload / Signature | Description |
| --- | --- | --- | --- |
| `workspace_open` | UI -> Host | `(path?: string) -> WorkspaceListing` | Open the active session workspace (when `path` is omitted) or resolve `path` via `SessionService::open_workspace`, switch `WorkspaceService` and `LspService`, and emit `session-updated`. |
| `session_get` | UI -> Host | `(sessionId?: string) -> Session` | Load `~/.lgtm/sessions/default.json` (including `sidebar_widget`, `projects`, and per-workspace `WorkspaceSessionState`). |
| `session_save` | UI -> Host | `(patch: WorkspaceStatePatch) -> Session` | Save the active `sidebar_widget`, `expanded_dirs`, and filtered `edited \|\| last_opened` tabs for a workspace to `default.json`. |
| `project_set_collapsed` | UI -> Host | `(projectId: string, collapsed: boolean) -> Session` | Collapse or expand the linked worktrees under a project in `default.json`. |
| `project_update_appearance` | UI -> Host | `(projectId: string, name?: string \| null, icon?: string \| null, color?: string \| null) -> Session` | Update a project's custom display name, icon, and color in `default.json`. |
| `project_rename_worktree` | UI -> Host | `(worktreePath: string, name: string) -> Session` | Rename a worktree section's display label in `default.json` without changing its Git branch or checkout path. |
| `project_create_worktree` | UI -> Host | `(projectId: string, branch: string, baseRef?: string) -> Session` | Create a `Managed` Git worktree under `~/.lgtm/worktrees/<project>/<branch-slug>` and attach it to the session, or return a typed creation error. |
| `project_remove_worktree` | UI -> Host | `(projectId: string, worktreePath: string, force?: boolean) -> Session` | Remove a worktree from the session, deleting checkout storage on disk when `WorktreeSource::Managed` (or returning a `dirty_worktree` error when `force` is false). |
| `project_remove` | UI -> Host | `(projectId: string) -> Session` | Remove a project and its workspace states from the `default` session. |
| `session-updated` | Host -> UI | `Session` | Push updated session state to the webview when native menus, CLI `lgtm open`, or `workspace_open` change the active workspace. |

## Implementation

### Package Layout

```text
apps/
├── lgtm-desktop/src/
│   ├── app.rs                         Registers SessionService in AppState and wires session_* / project_* IPC commands
│   ├── events.rs                      Routes WorkspaceOpened to LspService and emits session-updated
│   └── services/
│       └── workspace.rs               Edits the single active checkout directory (default spot or worktree)
└── lgtm-web/src/lib/
    ├── clients/workspaceClient.ts     Adds session_* and project_* IPC bindings
    ├── services/
    │   ├── hotkeyService.svelte.ts    Registers global shortcuts (Mod+Shift+B, Mod+B), WhichKey leader chords, and CommandMenu items
    │   └── workspaceService.svelte.ts Owns active workspace file tree, tabs (tab.edited), per-workspace cache, and session/project state
    └── components/
        ├── Shell.svelte               Mounts independent #projects-dock-pane and #explorer-dock-pane in the left dock and renders the SidebarFooter icon switcher
        ├── ContentPane.svelte         Renders the active workspace editor tabs, CodeEditor, and bottom status bar
        └── ProjectsPane.svelte        Renders compact collapsible project branches, worktree items, context menus, and modal dialogs
```

### Tab Retention State Machine

```mermaid
stateDiagram-v2
    [*] --> PreviewOpened: User clicks file in Explorer
    PreviewOpened --> DroppedOnSave: Another tab becomes active (edited = false, last_opened = false)
    PreviewOpened --> RetainedAsLastOpened: Remains active tab on workspace switch or quit (last_opened = true)
    PreviewOpened --> EditedInSession: User modifies buffer in CodeEditor (edited = true)
    EditedInSession --> RetainedAsEdited: Kept in default.json even after switching to other tabs
    EditedInSession --> ClosedByUser: User clicks tab close button [x]
    ClosedByUser --> [*]
```

## References

- [Session library (`lgtm-session`) architecture](../session/index.md).
- [Project library (`lgtm-project`) architecture](../project/index.md).
- [Rust utilities library (`lgtm-rsutils`) architecture](../rsutils/index.md).
- [ProjectsPane UI component](../web/projects-pane.md).
- [Desktop application architecture](index.md).
- [Desktop event handling and service orchestration](events.md).
