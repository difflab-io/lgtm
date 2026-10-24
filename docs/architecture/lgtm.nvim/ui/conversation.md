# Conversation view

## Overview

The conversation displays thread status, comments, replies, and original location evidence. A floating modal shows selected-line feedback. A persistent split shows all-file discussions.

## Design

### Layout

```text
+ Review thread -----------------------+
| OPEN / STALE / OUTSIDE DIFF            |
| Original evidence when needed         |
| TOP-LEVEL COMMENT / author / status   |
|   Body                                |
|   └─ REPLY 1 / author / status         |
|      Reply body                       |
+ Tab select / c e dd x / q close -------+
```

### Behavior

| State or action | Result |
| --- | --- |
| Tab / Shift-Tab | Select the next/previous comment by ID. |
| c / e / dd | Return reply/edit/delete intent to the controller. |
| x | Return resolution intent when the controller allows it. |
| Pane update | Keep the selected comment and focus. |
| Enter in split | Ask the controller to navigate without closing the pane. |
| Empty | Show No comments. |
| q / Esc | Close and restore a valid source window. |

## Implementation

### Public API

```lua
require("lgtm.ui.conversation").open({
  mode = "modal", -- or "split"
  threads = display_records,
  mappings = {},
  on_action = function(action, thread_id, comment_id) end,
  on_navigate = function(thread_id) end,
  on_change = function(view) end,
  on_ready = function() end,
})
-- Handle: selected(), select_comment(id), update(records), close().
```

### State and Data Flow

`threads.lua` builds display records and allowed actions from current application data. The component uses stable thread/comment IDs in NUI tree nodes. NuiComponents hosts the modal. NUI Split hosts the persistent pane. The controller rechecks ownership and resolves source locations before mutations or navigation.

### Styling and Responsiveness

Both hosts share semantic lines and selected-comment styling. Multiline comments wrap within the available width. The footer names the selected root or reply. The library owns borders and floating geometry.

### Maintainer Notes

Global edit/delete shortcuts wait for `on_ready` before sending an action. Otherwise, asynchronous mounting can send the key to the source buffer. Reuse `threads.spec.ts` for in-place resolution, stale cards, and outside-diff feedback. Full original evidence belongs here, not in compact cards.

See the [implementation](../../../../apps/lgtm.nvim/lua/lgtm/ui/conversation.lua) and [plugin architecture](../index.md).
