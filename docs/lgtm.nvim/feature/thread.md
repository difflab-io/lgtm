# Thread

## Overview

Compose a thread header, location evidence, and its comment components.

## Design

### Layout

```text
OUTDATED · OPEN · 1 reply
──────────────────────────────────────────────────
Original lines 20–22
> submit(body)
TOP-LEVEL COMMENT · Alex · Published
  Keep this discussion.
  └─ REPLY 1 · You · Local draft
     Agreed.
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Current
  Current --> Outdated: GitHub marks a selection outdated
  Current --> Resolved: Service publishes resolution
  Resolved --> Current: Service publishes reopening
```

### Public API

```lua
require("lgtm.features.thread").render({
  thread = thread,
  comments = comments,
  can_resolve = can_resolve,
  width = width,
  mode = "split",
  index = 1,
  count = 1,
})
```

The [threads pane](threads-pane.md) owns keys and dispatches actions. This component returns NUI tree nodes with stable thread/comment IDs.

### State and Data Flow

| Owner | State |
| --- | --- |
| Component | None. Rendering is a pure function of props; selection belongs to the host. |
| Service | The parent supplies a Thread, its comments, location status, and allowed actions from service data. |
| Backend | Saved feedback, original anchor evidence, and authoritative location/lifecycle decisions. |

```mermaid
flowchart TD
  Backend["LGTM backend APIs"] -->|"conversation / saved source coordinates"| Service["threads_service signals"]
  Service --> Pane["threads_pane"]
  Pane -->|"Thread / Comment props"| Thread["UI.thread"]
  Thread -->|"Comment props"| Comment["UI.comment"]
  Pane -->|"reply / edit / delete / resolve"| Service
  Service -->|"guarded writes"| Backend
```

## Implementation

- [Source](../../../apps/lgtm.nvim/lua/lgtm/features/thread.lua).
- [Overall service and backend wiring](../../architecture/lgtm.nvim/index.md).
