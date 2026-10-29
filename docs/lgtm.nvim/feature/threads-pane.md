# Threads pane

## Overview

Render a reactive conversation as a persistent pane or a selected-thread modal.

## Design

### Layout

```text
source buffer                              │ OPEN · 1 reply · file.lua:20
                                           │──────────────────────────────────────
                                           │ TOP-LEVEL COMMENT · Alex · Published
                                           │   Keep this discussion.
                                           │   └─ REPLY 1 · You · Local draft
                                           │      Agreed.
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Open
  Open --> Composing: Reply or edit
  Composing --> Open: Cancel
  Composing --> Closed: Save succeeds
  Open --> Open: Service publishes resolution
  Open --> Navigating: Enter in pane
  Navigating --> Open: Pane stays mounted
  Open --> Closed: q or Esc
```

### Public API

```lua
require("lgtm.features.threads_pane").open_selected(state, threads, pane, on_ready)
```

Tab / Shift-Tab select comments. `c` replies, `e` edits, `dd` requests deletion, and `x` toggles resolution. Enter in the pane navigates to code. `q` / Esc close the host. `setup.mappings` remaps these keys. Backend permissions still apply.

### State and Data Flow

| Owner | State |
| --- | --- |
| Component | Host mode, selected comment ID, scroll/focus position, and open/closed window. Selected thread IDs restrict a modal; the pane follows the full conversation. |
| Service | threads_service owns conversations, placements, request status, and guarded mutations. The pane does not retain its own comment list. |
| Backend | Saved feedback, original anchor evidence, and authoritative location/lifecycle decisions. |

```mermaid
flowchart TD
  Component["Threads pane<br/>props / UI interaction"]
  Component --> S0["threads_service<br/>service-owned data and operations"]
  Backend["LGTM backend APIs"]
  S0 --> Backend
  Backend -->|"accepted records / errors"| Signals["Service signals"]
  Signals -->|"declarative inputs"| Component
```

```mermaid
flowchart TD
  State["threads_service.data"] --> Pane["threads_pane"]
  Pane --> Thread["UI.thread(props)"]
  Thread --> Comment["UI.comment(props)"]
  Pane -->|"reply / edit / delete / resolve"| State
```

## Implementation

- [Source](../../../apps/lgtm.nvim/lua/lgtm/features/threads_pane.lua).
- [Overall service and backend wiring](../../architecture/lgtm.nvim/index.md).
