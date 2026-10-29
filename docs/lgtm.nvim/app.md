# Application orchestration

## Overview

Coordinate review tabs, notes actions, export presentation and the UI components that display service data.

## Design

### Layout

```text
Files                 │ old source                  │ new source
──────────────────────│─────────────────────────────│─────────────────────────────
✓ file.lua            │ submit(body)                │ submit(body, done)
  other.lua           │                             │╭─ Local draft · Open ─────╮
                      │                             ││ Keep failed-save text.   │
                      │                             │╰ Enter open · za expand ─╯
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Opening
  Opening --> Ready: Native buffers and service data ready
  Ready --> Refreshing: Refresh or PR update
  Refreshing --> Ready: New service snapshot
  Ready --> Closed: Lgtm close
```

### Public API

```lua
require("lgtm.app").open(state, on_loaded)
```

`:Lgtm refresh`, `files`, `threads`, `complete`, `submit`, `sync`, and `close` operate on the active review. Global keys use `setup.prefix`; component-local keys are documented by their feature pages.

### State and Data Flow

| Owner | State |
| --- | --- |
| Application | Tab/window lifecycle, selected file/side, modal visibility, and focus restoration. |
| Service | session_service owns review metadata; files_service owns file lists and marks; threads_service owns feedback; diff_service owns comparison/source data. |
| Backend | Saved feedback, original anchor evidence, and authoritative location/lifecycle decisions. |

```mermaid
flowchart TD
  Component["app.lua<br/>user intent / editor lifecycle"]
  Component --> S0["session_service<br/>service-owned data and operations"]
  Component --> S1["files_service<br/>service-owned data and operations"]
  Component --> S2["threads_service<br/>service-owned data and operations"]
  Component --> S3["diff_service<br/>service-owned data and operations"]
  Backend["LGTM backend APIs"]
  S0 --> Backend
  S1 --> Backend
  S2 --> Backend
  S3 --> Backend
  Backend -->|"accepted records / errors"| Signals["Service signals"]
  Signals -->|"declarative inputs"| Component
```

## Implementation

- [Source](../../apps/lgtm.nvim/lua/lgtm/app.lua).
- [Overall service and backend wiring](../architecture/lgtm.nvim/index.md).
