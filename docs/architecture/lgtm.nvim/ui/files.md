# Directory file pane

## Overview

This pane lists files for explicit directory reviews. Git-backed reviews keep CodeDiff's native explorer. The component consumes service-derived file rows from `features/files_pane.lua`.

## Design

### Layout

The directory pane sits to the left of the selected file. Its winbar names the selection and reviewed-mark keys. The fixed pane width can shorten the winbar text. Git reviews keep CodeDiff's own explorer.

Selected file with a reviewed checkmark:

```text
<s · Enter select · m reviewed│  8 -- review implementation
▶ ✓ M src/review.lua          │  9 -- review implementation
  · = src/notes.lua           │ 10 -- review implementation
  · M README.md               │ 11 -- review implementation
                              │ 12 -- review implementation
                              │ 13 -- review implementation
                              │ 14 -- review implementation
                              │ 15 -- review implementation
                              │ 16 -- review implementation
                              │ 17 -- review implementation
                              │ 18 -- review implementation
                              │ 19
                              │ 20 local function save(body)
                              │ 21   submit(body, done)
                              │ 22 end
                              │
```

The first glyph names the open file. The checkmark means reviewed. `M` means changed content, and `=` means unchanged content.

After the file changes, its reviewed mark clears. Selection stays on the same path.

```text
<s · Enter select · m reviewed│  8 -- review implementation
▶ · M src/review.lua          │  9 -- review implementation
  · = src/notes.lua           │ 10 -- review implementation
  · M README.md               │ 11 -- review implementation
                              │ 12 -- review implementation
                              │ 13 -- review implementation
                              │ 14 -- review implementation
                              │ 15 -- review implementation
                              │ 16 -- review implementation
                              │ 17 -- review implementation
                              │ 18 -- review implementation
                              │ 19
                              │ 20 local function save(body)
                              │ 21   submit(body, done)
                              │ 22 end
                              │
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Unreviewed: Open an unreviewed path
  [*] --> Reviewed: Open a reviewed path
  Unreviewed --> Reviewed: m / service persists mark
  Reviewed --> Unreviewed: m / service clears mark
  Reviewed --> Unreviewed: Content changes / service invalidates mark
  Unreviewed --> Unreviewed: Enter selects a path
  Reviewed --> Reviewed: Enter selects a path
  Unreviewed --> [*]: f or q
  Reviewed --> [*]: f or q
```

### Public API

Keyboard API:

| Keys | Mode | Result |
| --- | --- | --- |
| Enter | Normal | Open the selected path. |
| m | Normal | Toggle the reviewed mark for that path. |
| Tab / Shift-Tab | Normal | Navigate to the next/previous unreviewed file. |
| ]f / [f | Normal | Navigate to the next/previous file. |
| f / q | Normal | Hide the directory pane. |

Lua API:

```lua
require("lgtm.ui.files").open({
  data = file_rows, -- files_service signal combined with UI selection
  anchor_win = source_win,
  focus = true,
  on_select = function(path)
  end,
  on_toggle_reviewed = function(path)
  end,
  on_navigate = function(direction, unreviewed)
  end,
  on_close = function()
  end,
})
-- Handle: close().
-- Each item has these fields:
-- path
-- selected
-- reviewed
-- changed
```

### State and Data Flow

```mermaid
flowchart LR
  Store["files_service<br/>Files + reviewed marks"] --> Rows["File rows signal"]
  Selection["Files pane<br/>Selected path"] --> Rows
  Rows --> Tree["NUI file tree<br/>Node ID = path"]
  Tree -->|"Enter: path"| Pane["features/files_pane"]
  Tree -->|"m: path"| Pane
  Pane -->|"toggle reviewed"| Store
  Pane -->|"select file"| Source["Review workspace"]
```

## Implementation

- Use the path as the tree node ID so updates can retain the selected file.
- [Source](../../../../apps/lgtm.nvim/lua/lgtm/ui/files.lua).
