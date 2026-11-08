# Web & Desktop Frontend (`lgtm-web`) - software architecture

## Overview

`lgtm-web` is the Svelte 5 and SvelteKit frontend for LGTM, serving both the native Tauri v2 desktop IDE shell (`lgtm-desktop`) and the browser-accessible review interface (`lgtm-forge`). In desktop mode, `lgtm-web` composes portable Helical UI primitives from `@cloudvoyant/helical-svelte` and `lgtm-ui` (`CodeEditor`, `DiffEditor`, `TreeView`, `Tabs`, `Menu`, `ContextMenu`) with single-responsibility reactive services (`WorkspaceService`, `ReviewService`, `DiffService`, `LspService`, and `SettingsService`) over a typed Specta IPC client (`WorkspaceClient`), coordinating cross-service lifecycle events in `Shell.svelte`.

## Requirement

### Functional

- Reactive workspace and editor state (`WorkspaceService`): Browse workspace directories, manage open file and read-only diagnostics tabs, debounce editor buffer saves (`150ms`), open global (`~/.lgtm/config.json`) and local (`.lgtm/config.json`) configuration files as standard editor tabs (`Cmd+,` and `Cmd+Shift+,`), and flush pending saves on tab switch, workspace switch, or application exit.
- Review mode entry (`ReviewService`): Enter diff inspection in either `Local` mode (LGTM shadow commits in `~/.lgtm/snapshots/$repoSlug` plus uncommitted working-tree changes) or `ForgeBacked` mode (standard Git branch commits in `.git`), and coordinate with `DiffService` to populate the diff sidebar and editor.
- Single-file diff viewer, changed-file tree, and commit checklist (`DiffService`): Track the commit checklist (`commits` and `selectedCommitIds`) in `CommitSelector`, list changed files (`A`, `M`, `D`, `R`, `+additions`, `-deletions`) in `DiffExplorer`, open one file diff at a time in `DiffEditor`, and toggle between `unified` and `split` columns or between collapsed hunks (`showFullFile = false`) and the full file (`showFullFile = true`).
- Language server status, diagnostics, and navigation (`LspService`): Track per-language server readiness (`Starting`, `Ready`, `Unavailable`, `Stopped`) and file diagnostics (`lsp-status` and `lsp-diagnostics` events), forward live buffer edits (`lsp_did_change`), resolve hover documentation (`lsp_hover`) and go-to-definition locations (`lsp_goto_definition`), and open or refresh read-only diagnostics buffer tabs.
- File-driven user configuration (`SettingsService`): Load the resolved configuration snapshot (`config_get`), react to live `config-updated` events emitted whenever `~/.lgtm/config.json` or `<workspace>/.lgtm/config.json` is saved, synchronize the active color theme (`system`, `light`, `dark`) with the document root and native Tauri window chrome, expose editor preferences (`lineWrapping`), and persist recent workspace roots in `localStorage`.

### Non-Functional

- Single-responsibility services: Keep `WorkspaceService`, `ReviewService`, `DiffService`, `LspService`, and `SettingsService` focused on their respective domains, and wire cross-service event subscriptions (`lsp-status`, `lsp-diagnostics`, `config-updated`, `diff-updated`, `workspace-quit-requested`) in `Shell.svelte`.
- Isolated keybindings: Centralize frontend shortcut definitions in `src/lib/keybindings.ts` so they can register directly with Helical UI `which-key` and hotkey primitives.
- Generated IPC type parity: Consume Specta-generated TypeScript bindings (`src/lib/gen/bindings.ts`) inside `WorkspaceClient` so frontend services and Rust Tauri commands share compile-time contract checks.

## Design

### Usage

```ts
import { createWorkspaceClient } from '$lib/clients/workspaceClient'
import { DiffService } from '$lib/services/diffService.svelte'
import { LspService } from '$lib/services/lspService.svelte'
import { ReviewService } from '$lib/services/reviewService.svelte'
import { SettingsService } from '$lib/services/settingsService.svelte'
import { WorkspaceService } from '$lib/services/workspaceService.svelte'

const client = createWorkspaceClient()
const lsp = new LspService(client)
const settings = new SettingsService(client)
const workspace = new WorkspaceService(client, lsp, settings)
const diff = new DiffService(client, workspace)
const review = new ReviewService(client, diff)

await workspace.init()
await review.openDiffMode('local')
await diff.toggleFullFile()
```

### Components and Responsibilities

```mermaid
flowchart TB
    subgraph Components["Svelte 5 Components (src/lib/components)"]
        SHELL["Shell.svelte"]
        EXPLORER["WorkspaceExplorer.svelte"]
        DIFF_EXP["DiffExplorer.svelte & CommitSelector.svelte"]
        CONTENT["ContentPane.svelte (CodeEditor & DiffEditor)"]
        LSP_MENU["LspStatusMenu.svelte"]
        SHELL --> EXPLORER
        SHELL --> DIFF_EXP
        SHELL --> CONTENT
        SHELL --> LSP_MENU
    end

    subgraph Services["Reactive Services (src/lib/services)"]
        WS_SVC["WorkspaceService (workspaceService.svelte.ts)"]
        REV_SVC["ReviewService (reviewService.svelte.ts)"]
        DIFF_SVC["DiffService (diffService.svelte.ts)"]
        LSP_SVC["LspService (lspService.svelte.ts)"]
        SET_SVC["SettingsService (settingsService.svelte.ts)"]
        REV_SVC --> DIFF_SVC
        DIFF_SVC --> WS_SVC
        WS_SVC --> LSP_SVC
        WS_SVC --> SET_SVC
    end

    subgraph Client["IPC & Transport Layer (src/lib/clients)"]
        WS_CLIENT["WorkspaceClient / TauriWorkspaceClient (workspaceClient.ts)"]
        BINDINGS["Specta Bindings (src/lib/gen/bindings.ts)"]
        WS_CLIENT --> BINDINGS
    end

    SHELL --> WS_SVC
    SHELL --> REV_SVC
    SHELL --> DIFF_SVC
    SHELL --> LSP_SVC
    SHELL --> SET_SVC
    WS_SVC --> WS_CLIENT
    REV_SVC --> WS_CLIENT
    DIFF_SVC --> WS_CLIENT
    LSP_SVC --> WS_CLIENT
    SET_SVC --> WS_CLIENT
```

- `Shell.svelte`: Application entry point that instantiates `WorkspaceService`, `ReviewService`, `DiffService`, `LspService`, and `SettingsService`, subscribes to Tauri backend events (`lsp-status`, `lsp-diagnostics`, `config-updated`, `diff-updated`, `workspace-quit-requested`), synchronizes OS/window color scheme media queries with `SettingsService.theme`, and binds shortcuts from `src/lib/keybindings.ts`.
- `WorkspaceService` (`workspaceService.svelte.ts`): Owns the active workspace tree, expanded directories, open editor tabs, dirty buffer tracking, and `150ms` debounced save timer, delegates LSP notifications to `LspService`, and loads initial configuration through `SettingsService`.
- `ReviewService` (`reviewService.svelte.ts`): Owns the active review context mode (`mode: 'local' | 'forge_backed'`), enters diff viewing in either local shadow-commit mode or branch Git-commit mode, and prepares the hook point for future `lgtm-forge` review sessions and comments.
- `DiffService` (`diffService.svelte.ts`): Owns the reactive commit checklist (`commits`, `selectedCommitIds`), changed file list (`files`), active single-file diff payload (`activeFile`), layout mode (`viewMode: 'unified' | 'split'`), and full-file vs. hunks-only state (`showFullFile: boolean`). See [DiffExplorer & CommitSelector architecture](diff-explorer.md).
- `LspService` (`lspService.svelte.ts`): Owns reactive language server status maps and per-file UTF-16 diagnostic arrays, formats status indicators for `LspStatusMenu.svelte`, and queries hover/definition/diagnostics buffers via `WorkspaceClient`.
- `SettingsService` (`settingsService.svelte.ts`): Owns the latest `ResolvedConfigSnapshot` from `lgtm-config`, derives `theme` (`ThemeMode`), `lineWrapping` (`LineWrap`), `activeRoot`, and `recentRoots`, and updates `knownRoots` in `localStorage`.
- `WorkspaceClient` (`workspaceClient.ts`): Defines the transport-agnostic `WorkspaceClient` interface and `TauriWorkspaceClient` implementation over `@tauri-apps/api/core` and `src/lib/gen/bindings.ts`.

### Data and Control Flow

```mermaid
sequenceDiagram
    participant User as User (Shell.svelte / DiffExplorer / DiffEditor)
    participant RevSvc as ReviewService
    participant DiffSvc as DiffService
    participant Client as TauriWorkspaceClient
    participant Host as lgtm-desktop (app.rs)

    User->>RevSvc: Switch to Diffs in Local or ForgeBacked mode
    RevSvc->>DiffSvc: loadDiffState(mode)
    DiffSvc->>Client: diffGetState({ mode, commitIds: null })
    Client->>Host: invoke("diff_get_state", { mode, commitIds: null })
    Host-->>DiffSvc: DiffWorkspaceSnapshot { mode, commits, selectedCommitIds, files }
    DiffSvc-->>User: Render DiffExplorer file tree and bottom CommitSelector checklist

    User->>DiffSvc: Click file in DiffExplorer or toggle commit checkbox
    DiffSvc->>Client: diffReadFile({ relativePath, mode, commitIds })
    Client->>Host: invoke("diff_read_file", { relativePath, mode, commitIds })
    Host-->>DiffSvc: FileDiffContent { baseContent, headContent, isWorkingTree }
    DiffSvc-->>User: Render synced Unified or Split diff in DiffEditor
```

### Event Handling and Push Propagation

`lgtm-web` does not use an internal frontend event bus. UI state propagates synchronously through Svelte 5 `$state` runes and `$effect` blocks. Tauri event listeners in `workspaceClient.ts` are used strictly for unsolicited push notifications from `lgtm-desktop` (`AppEvents::emit_to_webview`) and native window close hooks, wired in `Shell.svelte` `onMount`.

```mermaid
flowchart TD
    subgraph HostPush["1. Host Push Event (workspaceClient.ts)"]
        E_LSP["'lsp-status' / 'lsp-diagnostics'"]
        E_CFG["'config-updated'"]
        E_DIFF["'diff-updated'"]
        E_MENU["'workspace-open-file'"]
        E_QUIT["'workspace-quit-requested' / Window.onCloseRequested"]
    end

    subgraph ShellMount["2. Listener Wiring (Shell.svelte onMount)"]
        H_LSP["registerLspHandlers"]
        H_CFG["registerConfigHandlers"]
        H_DIFF["registerDiffHandlers"]
        H_QUIT["registerQuitHandlers"]
    end

    subgraph Services["3. Reactive Service State ($state)"]
        S_LSP["LspService.handleStatus / handleDiagnostics"]
        S_SET["SettingsService.handleUpdated(snapshot)"]
        S_DIFF["DiffService.handleUpdated(snapshot)"]
        S_WS["WorkspaceService.openPath / flushSave"]
    end

    subgraph UI["4. Reactive DOM & Host Sync"]
        U_EDITOR["CodeEditor & DiffEditor squiggles, lineWrapping, & Diagnostics tab"]
        U_DIFF["DiffExplorer file tree & CommitSelector checklist"]
        U_MENU["LspStatusMenu & TreeView badges"]
        U_THEME["$effect(settings.theme) -> ThemeService.setPreference"]
        U_EXIT["workspaceClient.finishQuit() / Window.destroy()"]
    end

    E_LSP --> H_LSP --> S_LSP
    S_LSP --> U_EDITOR
    S_LSP --> U_MENU

    E_CFG --> H_CFG --> S_SET
    S_SET --> U_THEME
    S_SET --> U_EDITOR

    E_DIFF --> H_DIFF --> S_DIFF --> U_DIFF

    E_MENU --> H_CFG --> S_WS --> U_EDITOR
    E_QUIT --> H_QUIT --> S_WS --> U_EXIT
```

| Listener Registration (`workspaceClient.ts`) | Subscribed Events | Payload Shape | Target Service Call (`Shell.svelte`) |
| --- | --- | --- | --- |
| `registerLspHandlers` | `"lsp-status"`<br/>`"lsp-diagnostics"` | `LspServerStatusEvent`<br/>`LspFileDiagnosticsEvent` | `workspace.handleLspStatus(event)`<br/>`workspace.handleLspDiagnostics(event)` |
| `registerConfigHandlers` | `"config-updated"`<br/>`"workspace-open-file"` | `ResolvedConfigSnapshot`<br/>`FileOpenedEvent` | `workspace.settings.handleUpdated(snapshot)`<br/>`workspace.openPath(event.path, { title: event.title, focus: event.focus })` |
| `registerDiffHandlers` | `"diff-updated"` | `DiffWorkspaceSnapshot` | `diff.handleUpdated(snapshot)` |
| `registerQuitHandlers` | `"workspace-quit-requested"`<br/>`Window.onCloseRequested` | `void`<br/>`CloseRequestedEvent` | `await workspace.flushSave()` -> `finishQuit()` / `destroy()` |

### Interfaces and Contracts

| Service / Method | Signature | Description |
| --- | --- | --- |
| `WorkspaceService.init` | `init(): Promise<void>` | Load initial configuration snapshot, open the remembered or default workspace root, and mark the frontend ready for quit handshakes. |
| `WorkspaceService.openWorkspace` | `openWorkspace(path?: string): Promise<void>` | Flush pending saves, open the target workspace directory, refresh `LspService` and `DiffService`, and record the root in `SettingsService`. |
| `WorkspaceService.selectFile` | `selectFile(relativePath: string): Promise<void>` | Read a workspace file, open or focus its editor tab, and synchronize LSP status. |
| `WorkspaceService.openPath` | `openPath(path: string, options?: OpenPathOptions): Promise<void>` | Open or focus any workspace file or configuration file (`~/.lgtm/config.json` or `.lgtm/config.json`) via `workspace_read`, attach `vscode-json-language-server` asynchronously, and refresh configuration when opening config files. |
| `WorkspaceService.scheduleSave` / `flushSave` | `scheduleSave(content: string): void` / `flushSave(): Promise<void>` | Forward live `didChange` updates to `LspService` and persist dirty buffers after `150ms` or before navigation/quit. |
| `ReviewService.openDiffMode` | `openDiffMode(mode: ReviewMode): Promise<void>` | Set the active review diff mode (`'local'` or `'forge_backed'`), switch the sidebar to `DiffExplorer`, and load commits and changed files through `DiffService`. |
| `DiffService.loadState` | `loadState(mode: ReviewMode, commitIds?: string[]): Promise<void>` | Fetch `DiffWorkspaceSnapshot` (`commits`, `selectedCommitIds`, `files`) for the active workspace and review mode. |
| `DiffService.toggleCommit` | `toggleCommit(commitId: string): Promise<void>` | Toggle a commit checkbox in `CommitSelector`, refresh `files` across `selectedCommitIds`, and reload the active file in `DiffEditor`. |
| `DiffService.openFileDiff` | `openFileDiff(relativePath: string): Promise<void>` | Fetch `FileDiffContent` (`baseContent`, `headContent`) for `relativePath` across `selectedCommitIds` and open it in `DiffEditor`. |
| `DiffService.nextFile` / `prevFile` | `nextFile(): Promise<void>` / `prevFile(): Promise<void>` | Step to the next or previous changed file in `DiffExplorer` and load its diff into `DiffEditor`. |
| `DiffService.toggleViewMode` / `toggleFullFile` | `toggleViewMode(): void` / `toggleFullFile(): void` | Toggle `DiffEditor` between `'unified'` and `'split'` columns or between collapsed hunks (`showFullFile = false`) and the full file (`showFullFile = true`). |
| `LspService.refresh` | `refresh(): Promise<void>` | Fetch the current `LspWorkspaceSnapshot` (`servers` and `diagnostics`) for the active workspace. |
| `LspService.hover` / `gotoDefinition` | `hover(path: string, offset: number)` / `gotoDefinition(path: string, offset: number)` | Query LSP hover Markdown or definition locations at a UTF-16 code-unit offset. |
| `SettingsService.load` | `load(): Promise<ResolvedConfigSnapshot \| null>` | Fetch the latest `ResolvedConfigSnapshot` from `config_get` and update reactive preferences. |
| `SettingsService.handleUpdated` | `handleUpdated(next: ResolvedConfigSnapshot): void` | Apply a live `config-updated` event payload when `~/.lgtm/config.json` or `.lgtm/config.json` is saved. |

## References

- [DiffExplorer & CommitSelector UI components](diff-explorer.md).
- [DiffEditor UI component (`libs/lgtm-ui`)](../ui/diff-editor.md).
- [Desktop diff viewer and commit inspection](../desktop/diffs.md).
- [Desktop application (`lgtm-desktop`) architecture](../desktop/index.md).
- [Desktop event handling & service orchestration](../desktop/events.md).
- [Configuration library (`lgtm-config`) architecture](../config/index.md).
- [LSP library (`lgtm-lsp`) architecture](../lsp/index.md).
- [End-to-end testing architecture](../e2e/index.md).
- [Application architecture](../index.md).
