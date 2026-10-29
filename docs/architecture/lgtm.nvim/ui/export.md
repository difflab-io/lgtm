# Export widget

## Overview

Present Markdown already produced by a service. This widget does not load review records.

## Design

### Layout

```text
Export review
  Preview
  Save file
  Copy to clipboard
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Choosing
  Choosing --> Preview: Preview
  Choosing --> Destination: Save file
  Choosing --> Copied: Copy to clipboard
  Destination --> ConfirmOverwrite: Destination exists
  ConfirmOverwrite --> Saved: Overwrite
  ConfirmOverwrite --> [*]: Cancel
  Destination --> Saved: New destination
  Preview --> [*]: q
```

### Public API

```lua
require("lgtm.ui.export").open(
  markdown,
  "review"
)
```

Preview uses a read-only split; `q` closes it. Choice and destination prompts use the configured `vim.ui` providers.

### State and Data Flow

```mermaid
flowchart LR
  Backend --> Service["session_service.get_export"]
  Service --> App["app.lua"]
  App -->|"Markdown / title"| Widget["ui.export"]
  Widget --> Split["Preview split"]
  Widget --> File["Selected output file"]
  Widget --> Clipboard
```

Only transient prompt and preview-window state belongs to the widget.

## Implementation

- [Source](../../../../apps/lgtm.nvim/lua/lgtm/ui/export.lua).
