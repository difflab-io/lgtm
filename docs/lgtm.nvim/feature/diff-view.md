# CodeDiff view

## Overview

Show a saved review revision using CodeDiff layouts and read-only source panes. Git reviews use native revision buffers; directory and remote views use the existing file adapter with backend snapshot content.

## Design

### Layout

```mermaid
flowchart LR
  subgraph Review["LGTM review · saved revision"]
    direction LR
    subgraph Explorer[Files]
      File["✓ file.lua<br/>  other.lua"]
    end
    subgraph Old["old · file.lua"]
      Before["return old"]
    end
    subgraph New["new · file.lua"]
      After["return new"]
      Card["Local draft · Open<br/>Check this result<br/>Enter open · za expand"]
      After ~~~ Card
    end
    Explorer ~~~ Old
    Old ~~~ New
  end
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Loading
  Loading --> Ready: CodeDiff and conversation ready
  Ready --> Switching: Select file or layout
  Switching --> Ready: Native panes settle
  Ready --> Historical: Navigate older feedback
  Historical --> Ready: Refresh selects latest revision
  Ready --> Empty: Refreshed comparison has no changes
  Empty --> Ready: Capture a changed revision
  Ready --> Closed: Close review view
```

## Implementation

### Public API

```lua
require("lgtm.features.diff_view").open_native(state)
```

`:Lgtm layout` toggles the supported CodeDiff layout. `c` in normal or visual mode opens a comment composer. `:Lgtm refresh` captures saved source again; changing an ordinary editing buffer does not change an open snapshot. The source panes remain read-only. Closing the view keeps the session and feedback.

### State and Data Flow

```mermaid
flowchart LR
  Backend[ReviewService and SnapshotService] --> Session[session_service: current revision and history]
  Session --> View[diff_view: tab, panes, layout and selected revision]
  View --> CodeDiff[CodeDiff: two saved commits]
  Backend --> Files[files_service: file metadata and marks]
  Files --> Explorer[Native explorer]
  Backend --> Threads[threads_service: conversation and visible coordinates]
  Threads --> Cards[annotation_overlay]
  View -->|Comment intent| Threads
```

- [Source](../../../apps/lgtm.nvim/lua/lgtm/features/diff_view.lua).
- [Overall architecture](../../architecture/lgtm.nvim/index.md).
