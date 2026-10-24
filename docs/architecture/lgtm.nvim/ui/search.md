# Searchable selection

## Overview

The PR and session pickers show searchable candidates. Controllers supply labels and opaque IDs. This component does not load reviews or inspect repository data.

## Design

### Layout

```text
+ Search pull requests ----------------+
| #123 Search text                      |
+--------------------------------------+
| #123 Review title                     |
| #124 Another review                   |
+ Enter select / Esc cancel ------------+
```

### Behavior

| State or action | Result |
| --- | --- |
| --- | --- |
| Open | Focus the search input in insert mode. |
| Type | Filter display labels without changing candidate IDs. |
| Arrows / Ctrl-N / Ctrl-P | Move through matches using the library tree. |
| Enter | Close and return the focused ID. |
| No matches | Keep the picker open and ignore selection. |
| Esc | Close without selecting and restore source focus. |

## Implementation

### Public API

```lua
require("lgtm.ui.search").open({
  title = "Reviews",
  items = { { id = "review-id", label = "Review title" } },
  on_select = function(id) end,
})
```

### State and Data Flow

NuiComponents TextInput and Select share a data signal. A signal is a value that updates subscribed components. `sessions.lua` maps returned IDs to its candidates. The view owns only query and selection. It schedules filtering outside the text-change callback before updating the tree.

### Styling and Responsiveness

The renderer centers the picker and sizes it to the editor. A resize hook calls the upstream size API. Semantic LGTM highlights style focused options without changing other plugins.

### Maintainer Notes

Wait for the query buffer and insert mode before sending test input. Renderer mounting is asynchronous. Reuse PR selection and empty-search cases in the editor suite. Do not assert exact cell positions.

See the [implementation](../../../../apps/lgtm.nvim/lua/lgtm/ui/search.lua) and [plugin architecture](../index.md).
