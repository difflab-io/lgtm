# Comment cards

## Overview

Cards summarize feedback beside source code. They return highlighted virtual lines rather than opening a window. The annotation adapter places those lines in the correct source buffer.

## Design

### Layout

```text
+ Stale / anchor lost / 1 reply --------+
| Comment preview                       |
+ Author / Enter open / za expand -------+
```

### Behavior

| State or action | Result |
| --- | --- |
| Collapsed | Show status, reply count, author, and a compact preview. |
| Expanded | Show the full supplied conversation text. |
| Resolved and collapsed | Show one resolved-status row. |
| Lost anchor | Show a compact status and preview. The full view retains location evidence. |

## Implementation

### Public API

```lua
local rows = require("lgtm.ui.cards").render({
  resolved = false, file = false, status = "Local draft",
  location_status = nil,
  comments = { { author = "Reviewer", body = "Comment", is_reply = false } },
}, width, expanded)
```

### State and Data Flow

`threads.lua` maps domain comments into display records. NUI Line/Text compose highlighted chunks. The result is a list of Neovim virtual-line chunks. The adapter owns extmarks, source-side placement, and CodeDiff padding. `codediff.other_line` translates alignment positions.

### Styling and Responsiveness

Cards fit the current source-pane width and use semantic `LgtmComment*` links. The border follows `setup.ui.border`. Unicode width handling stays local because these lines decorate an existing source buffer.

### Maintainer Notes

Keep original ranges and excerpts in the conversation view. Cards do not infer anchors or select source lines. Reuse existing annotation, layout-switch, and lost-anchor journeys. NUI does not replace CodeDiff's source alignment.

See the [implementation](../../../../apps/lgtm.nvim/lua/lgtm/ui/cards.lua) and [plugin architecture](../index.md).
