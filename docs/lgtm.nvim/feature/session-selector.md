# Session selector

## Overview

Resume an active or completed local or forge review.

## Design

### Layout

```text
Search Local / GitHub reviews
╭──────────────────────────────────────────────────╮
│ feature                                          │
├──────────────────────────────────────────────────┤
│ feature [active] 2026-10-28 09:00                 │
│ PR #248 [completed] 2026-10-27 16:20              │
╰ Enter select · Esc cancel ───────────────────────╯
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Loading
  Loading --> Searching: Catalog arrives
  Searching --> Workspace: Select review
```

### Public API

```lua
require("lgtm.features.session_selector").select(completed)
```

`:Lgtm sessions` lists active reviews; `:Lgtm sessions completed` lists completed reviews. Enter opens the selected review; Esc cancels.

### State and Data Flow

| Owner | State |
| --- | --- |
| Component | Search query, focused result, and open/closed picker. |
| Service | The review catalog and loading/error state belong to session_service. |
| Backend | Saved feedback, original anchor evidence, and authoritative location/lifecycle decisions. |

```mermaid
flowchart TD
  Component["Session selector<br/>props / UI interaction"]
  Component --> S0["session_service<br/>service-owned data and operations"]
  Backend["LGTM backend APIs"]
  S0 --> Backend
  Backend -->|"accepted records / errors"| Signals["Service signals"]
  Signals -->|"declarative inputs"| Component
```

## Implementation

- [Source](../../../apps/lgtm.nvim/lua/lgtm/features/session_selector.lua).
- [Overall service and backend wiring](../../architecture/lgtm.nvim/index.md).
