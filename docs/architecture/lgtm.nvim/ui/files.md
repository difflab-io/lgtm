# Directory file pane

## Overview

This pane lists files for explicit directory reviews. Git-backed reviews keep CodeDiff's native explorer. The component receives paths and display flags from `files.lua`.

## Design

### Layout

```text
+ LGTM files --------+
| ▶ ✓ M src/a.lua    |
|   · = src/b.lua    |
|   · M plan.md      |
+ Enter / m reviewed +
```

### Behavior

| State or action | Result |
| --- | --- |
| Open/update | Render current paths and restore selection by path. |
| Enter | Return the selected path. |
| m | Request a reviewed-mark change for that path. |
| Tab / Shift-Tab | Request next/previous unreviewed file. |
| ]f / [f | Request next/previous file. |
| f / q | Ask the controller to hide the pane. |

## Implementation

### Public API

```lua
require("lgtm.ui.files").open({
  anchor_win = source_win, focus = true,
  on_select = function(path) end,
  on_toggle_reviewed = function(path) end,
  on_navigate = function(direction, unreviewed) end,
  on_close = function() end,
})
-- Handle: update(items, selected_path), close().
-- Item: { path, selected, reviewed, changed }.
```

### State and Data Flow

NUI Split owns the pane and NUI Tree owns its rows. Node IDs are file paths. The controller owns traversal order, selected content, mark persistence, and invalidation after content changes. A row number never becomes a persistence identifier.

### Styling and Responsiveness

The pane opens left of the source window with a fixed width. NUI manages the split lifecycle. Semantic glyphs distinguish selection, reviewed state, and changed content.

### Maintainer Notes

Reuse the existing ignored-directory navigation and reviewed-mark journey. Do not replace or duplicate CodeDiff's Git explorer. This component performs no filesystem reads or RPCs.

See the [implementation](../../../../apps/lgtm.nvim/lua/lgtm/ui/files.lua) and [plugin architecture](../index.md).
