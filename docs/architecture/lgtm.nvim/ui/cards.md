# Comment cards

## Overview

Cards summarize feedback beside source code. They return highlighted virtual lines rather than opening a window. The annotation adapter places those lines in the correct source buffer.

## Design

### Layout

Cards add display rows above their attached source line. They do not change the file. A collapsed card shows status, a preview, its author, and the open/expand keys.

```text
 20 local function save(body)
    ╭─ Local draft · Open · 1 reply                                                               ╮
    │ Keep the draft after a failed save.                                                         │
    ╰─Alex · Enter open · za expand                                                               ╯
 21   submit(body, done)
```

After `za`, the card shows all supplied comment bodies. Enter opens the conversation with comment selection and actions.

```text
 20 local function save(body)
    ╭─ Local draft · Open · 1 reply                                                               ╮
    │ Alex                                                                                        │
    │ Keep the draft after a failed save.                                                         │
    │ You                                                                                         │
    │ Agreed. This now retains the text.                                                          │
    ╰─Enter open · za collapse                                                                    ╯
 21   submit(body, done)
```

A lost anchor adds `Stale · anchor lost` or `Outdated · anchor lost` to the heading. The card stays compact. The conversation shows the original range and source excerpt. A resolved, collapsed card uses one row:

```text
 Resolved · Enter open · za expand                                                   ✓
```

### Behavior

```mermaid
stateDiagram-v2
  [*] --> Compact: Open discussion
  Compact --> Expanded: za
  Expanded --> Compact: za
  Compact --> ResolvedRow: Controller reports resolved
  ResolvedRow --> Expanded: za
  Expanded --> ResolvedRow: za while resolved
  Compact --> LostAnchor: Controller reports stale or outdated
  LostAnchor --> Conversation: Enter or K
  Compact --> Conversation: Enter or K
  Expanded --> Conversation: Enter or K
  Conversation --> [*]: Return to source / retain card state
```

### Public API

Keyboard API:

| Keys | Mode | Result |
| --- | --- | --- |
| Enter / K | Normal, on commented source | Open the conversation at the current line. |
| za | Normal | Expand/collapse discussions at the current line. Fall back to normal folding when none exist. |
| zA | Normal | Toggle expansion for each discussion in this file. |
| gH | Normal | Toggle comment-card visibility. |
| ]c / ]n | Normal | Jump to the next feedback location. |
| [c / [n | Normal | Jump to the previous feedback location. |
| x | Normal | Resolve/reopen a single thread at the line, or open the chooser when several exist. Permissions still apply. |
| dd | Normal, on commented source | Open the conversation. It does not immediately delete the comment. |

These keys belong to `features/annotation_overlay.lua`, which hosts the card in the source buffer. `setup.mappings` can replace them. Away from feedback, Enter/K and normal editing/folding actions keep their existing behavior.

Lua API:

```lua
local rows = require("lgtm.ui.cards").render(
  {
    resolved = false,
    file = false,
    status = "Local draft",
    location_status = nil,
    comments = {
      {
        author = "Reviewer",
        body = "Comment",
        is_reply = false,
      },
    },
  },
  width,
  expanded
)
```

### State and Data Flow

```mermaid
flowchart TD
  Props["Card + width + expanded"] --> Mode{"Resolved and collapsed?"}
  Mode -->|"Yes"| Summary["Resolved summary"]
  Mode -->|"No"| Header["Header"]
  Mode -->|"No"| Body["Preview or conversation body"]
  Mode -->|"No"| Footer["Footer"]
  Header --> Sections["Ordered NUI text sections"]
  Body --> Sections
  Footer --> Sections
  Summary --> Sections
  Sections --> Adapter["One converter to Neovim virtual lines"]
  Adapter --> Placement["annotation_overlay + diff_view.other_line<br/>Backend placements and opposite-side padding"]
```

## Implementation

- Cards decorate existing CodeDiff buffers. Keep the NUI-text-to-virtual-line conversion at one boundary.
- Translate opposite-pane padding through `codediff.other_line`; do not duplicate edit-range mapping here.
- [Source](../../../../apps/lgtm.nvim/lua/lgtm/ui/cards.lua).
