# Draft composer

## Overview

The composer edits notes, comments, replies, and final review bodies. Its caller supplies persistence. The component manages text and pending-save state.

## Design

### Layout

```text
+ Comment -----------------------------+
| Multiline Markdown text               |
|                                      |
+ Ctrl-S save / q or Esc cancel ---------+
```

### Behavior

| State or action | Result |
| --- | --- |
| Open | Show the initial or retained text. |
| Ctrl-S | Capture text, hide the view, and send once while pending. |
| Save succeeds | Close and release the retained draft. |
| Save fails | Restore editable text and show the error. |
| q / Esc in normal mode | Retain text and close without sending. |
| Insert mode | Use normal editor text entry and Escape to leave insert mode. |

## Implementation

### Public API

```lua
require("lgtm.ui.composer").open("Comment", "", function(body, done)
  -- The feature performs its save and calls done(err).
end, { action = "save", draft_id = "optional-owner-key" })
```

### State and Data Flow

The view keeps body and pending state in a NuiComponents signal. `M.drafts` retains cancelled or failed text independently of mounted windows. The caller can supply `draft_id`. Otherwise, the key uses source buffer, title, and initial text. The feature decides whether a save means local persistence, a private GitHub draft, or final submission.

### Styling and Responsiveness

A small TextInput subclass keeps the composer in normal mode on open. Markdown syntax and wrapping stay enabled. Borders and title/footer text come from `ui/theme.lua`. The renderer handles floating layout.

### Maintainer Notes

The pinned TextInput rejects a `linebreak` prop. Supply it through popup `win_options`. Save tests wait for the view close acknowledgement, because hiding a NUI layout also closes its windows. Existing journeys cover durable saves. One integration case covers failed text retention and duplicate-save suppression.

See the [implementation](../../../../apps/lgtm.nvim/lua/lgtm/ui/composer.lua) and [plugin architecture](../index.md).
