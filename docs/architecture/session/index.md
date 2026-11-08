# Session library (`lgtm-session`) - software architecture

## Overview

`lgtm-session` manages workspace sessions and persistence for LGTM. A session owns the open projects, the active sidebar view, the active workspace directory, and the remembered editor tabs and explorer state for each project checkout and worktree.

## Requirement

### Functional

- Store the ordered list of open projects and their worktrees in a named session.
- Track the active workspace directory and the active left sidebar view.
- Remember state separately for each project checkout and each worktree, including its active tab, tabs modified during the session, and expanded file-explorer directories.
- Retain only tabs modified during the session and the last active tab when a workspace is saved or switched.
- Delete worktree disk storage when removing a managed worktree, and only detach the sidebar entry when removing an external worktree.

### Non-Functional

- Switch active workspaces in memory without waiting on disk writes.
- Debounce and write session state atomically in the background so crashes or power loss do not corrupt saved sessions.
- Read and write sessions locally without network access or a background daemon.

## Design

### Components and Responsibilities

```mermaid
flowchart TB
    DESKTOP["lgtm-desktop (app.rs)"]
    CLI["lgtm-cli (future session attach/switch)"]

    subgraph SessionLib["libs/lgtm-session"]
        SVC["SessionService"]
        STORE["SessionStore (~/.lgtm/sessions/<id>.json)"]
        MODELS["Session, SidebarWidget, WorkspaceSessionState, PersistedTab, TabCandidate"]
        ERR["LgtmSessionError"]
        SVC --> STORE
        SVC --> MODELS
        SVC --> ERR
    end

    subgraph ProjectLib["libs/lgtm-project"]
        PROJ["ProjectService, Project, Worktree, WorktreeSource"]
    end

    subgraph RsUtils["libs/lgtm-rsutils"]
        ATOMIC["paths::atomic_write"]
    end

    DESKTOP --> SVC
    CLI -.-> SVC
    SVC --> PROJ
    STORE --> ATOMIC
    ATOMIC --> DISK[("~/.lgtm/sessions/default.json\n~/.lgtm/sessions/<name>.json")]
```

- `SessionService`: Coordinates `SessionStore` and `lgtm_project::ProjectService` to load the `default` session, register projects and worktrees, toggle collapsed project groups, switch the active workspace, and save filtered workspace tabs.
- `Session`: Top-level persisted root owning `id`, `sidebar_widget`, `active_workspace_root`, `projects`, and `workspaces`.
- `WorkspaceSessionState`: Per-workspace UI snapshot storing the retained `tabs` (`Vec<PersistedTab>`), `last_opened_tab_path`, and `expanded_dirs` for a single checkout path (either `Project.root_path` or a `Worktree.path`).
- `PersistedTab` and `TabCandidate`: Tab descriptors that enforce the retention rule (`edited || last_opened`).

```mermaid
erDiagram
    SESSION ||--o{ PROJECT : "owns ordered projects"
    SESSION ||--o{ WORKSPACE_SESSION_STATE : "remembers state per checkout path"
    PROJECT ||--o{ WORKTREE : "open linked worktrees"
    PROJECT ||--o| WORKSPACE_SESSION_STATE : "root_path edited as workspace"
    WORKTREE ||--o| WORKSPACE_SESSION_STATE : "path edited as workspace"
    WORKSPACE_SESSION_STATE ||--o{ PERSISTED_TAB : "retains edited or last_opened"

    SESSION {
        string id PK "default"
        SidebarWidget sidebar_widget "Explorer | Projects"
        string active_workspace_root "Path of active project root or linked worktree"
        timestamp updated_at
    }

    PROJECT {
        uuid id PK
        string name "Repository basename"
        string root_path UK "Primary checkout path"
        string branch "Checked-out branch at root_path"
        bool collapsed "True when child worktrees are collapsed"
    }

    WORKTREE {
        string path PK "Linked worktree directory path"
        string name "Worktree branch label"
        string branch "Checked-out Git branch"
        WorktreeSource source "Managed (~/.lgtm/worktrees) | External (~/.herdr/worktrees)"
    }

    WORKSPACE_SESSION_STATE {
        string workspace_root PK "Matches Project.root_path or Worktree.path"
        string last_opened_tab_path "Active tab path when last focused"
        string_array expanded_dirs "Expanded directory paths in WorkspaceExplorer"
    }

    PERSISTED_TAB {
        string path PK "Workspace-relative or configuration file path"
        string title "Tab display title"
        bool edited "True if modified during session"
        bool last_opened "True if active when workspace saved"
    }
```

### Data and Control Flow

#### 1. Ownership Hierarchy (`Session` -> `Project` -> `Workspace`)

```mermaid
flowchart TD
    SESS["Session ('default')\n~/.lgtm/sessions/default.json\nsidebar_widget = Projects\nactive_workspace_root = /Users/me/.herdr/worktrees/lgtm/feature-projects"]

    subgraph Projects["Session.projects (lgtm-project)"]
        P1["Project: lgtm (/Projects/lgtm, branch = main, collapsed = false)"]
        W_EXT["External Worktree: feature-projects (~/.herdr/worktrees/lgtm/feature-projects)"]
        W_MGD["Managed Worktree: feature-ui (~/.lgtm/worktrees/lgtm/feature-ui)"]
        P1 --> W_EXT
        P1 --> W_MGD
    end

    subgraph Workspaces["Session.workspaces (per-checkout editor state)"]
        WS_MAIN["WorkspaceSessionState (/Projects/lgtm)\ntabs: [README.md (last_opened)]"]
        WS_FEAT["WorkspaceSessionState (~/.herdr/worktrees/lgtm/feature-projects)\ntabs: [src/lib.rs (edited), src/app.rs (last_opened)]"]
    end

    SESS --> Projects
    SESS --> Workspaces
    P1 -.->|"root_path"| WS_MAIN
    W_EXT -.->|"path (ACTIVE)"| WS_FEAT
```

#### 2. Tab Retention Filter on Workspace Save or Switch

```mermaid
flowchart LR
    OPEN["Open Editor Tabs in Workspace\n1. Cargo.toml (viewed only)\n2. src/lib.rs (edited = true)\n3. src/app.rs (last_opened = true)"]
    RULE{"edited == true\nOR\nlast_opened == true?"}
    DISCARD["Discard Cargo.toml"]
    PERSIST["Save in WorkspaceSessionState.tabs\n- src/lib.rs (edited)\n- src/app.rs (last_opened)"]

    OPEN --> RULE
    RULE -->|"No"| DISCARD
    RULE -->|"Yes"| PERSIST
```

### Interfaces and Contracts

| Type / Method | Signature | Description |
| --- | --- | --- |
| `SessionService::new` | `fn new(lgtm_home: Option<PathBuf>) -> Result<Self>` | Create a session service rooted at `$LGTM_HOME/sessions` or `~/.lgtm/sessions`. |
| `SessionService::get_session` | `fn get_session(&self, session_id: &str) -> Result<Session>` | Load `~/.lgtm/sessions/<session_id>.json` or initialize the `default` session. |
| `SessionService::open_workspace` | `fn open_workspace(&self, session_id: &str, path: &Path) -> Result<Session>` | Resolve `path` via `lgtm-project`, upsert its parent `Project` and `Worktree`, and set `active_workspace_root`. |
| `SessionService::save_workspace_state` | `fn save_workspace_state(&self, session_id: &str, workspace_root: &Path, tabs: Vec<TabCandidate>, sidebar_widget: Option<SidebarWidget>) -> Result<Session>` | Filter `tabs` to `edited \|\| last_opened`, update `workspaces[workspace_root]`, and write `default.json`. |
| `SessionService::create_project_worktree` | `fn create_project_worktree(&self, session_id: &str, project_id: Uuid, branch: &str, base_ref: Option<&str>) -> Result<(Worktree, Session)>` | Create a `Managed` worktree at `~/.lgtm/worktrees/<project>/<branch-slug>`, attach it to `Session`, and activate it. |
| `SessionService::set_project_collapsed` | `fn set_project_collapsed(&self, session_id: &str, project_id: Uuid, collapsed: bool) -> Result<Session>` | Persist the collapsed or expanded worktree state for a project in `Session`. |
| `SessionService::remove_project_worktree` | `fn remove_project_worktree(&self, session_id: &str, project_id: Uuid, worktree_path: &Path, force: bool) -> Result<Session>` | Remove the worktree from `Session` and delete disk storage only when `WorktreeSource::Managed`. |
| `SessionService::remove_project` | `fn remove_project(&self, session_id: &str, project_id: Uuid) -> Result<Session>` | Remove a project and its workspace states from `Session` without deleting `External` worktrees. |

## Implementation

### Package Layout

```text
libs/lgtm-session/
├── Cargo.toml
├── mise.toml
├── src/
│   ├── lib.rs                 Public SessionService, Session, WorkspaceSessionState, and PersistedTab exports
│   ├── error.rs               LgtmSessionError and Result alias
│   ├── models/
│   │   ├── mod.rs
│   │   └── session.rs         Session, SidebarWidget, WorkspaceSessionState, PersistedTab, and TabCandidate
│   └── services/
│       ├── mod.rs
│       └── session.rs         SessionService orchestration and atomic ~/.lgtm/sessions/<id>.json persistence
└── tests/
    └── session.rs             BDD integration suite for session restore, worktree switching, and tab filtering
```

## References

- [Project library (`lgtm-project`) architecture](../project/index.md).
- [Rust utilities library (`lgtm-rsutils`) architecture](../rsutils/index.md).
- [Desktop project and session management](../desktop/projects.md).
- [ProjectsPane UI component](../web/projects-pane.md).
