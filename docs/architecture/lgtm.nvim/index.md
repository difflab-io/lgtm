# lgtm.nvim architecture

## Overview

LGTM adds saved-file notes and local/GitHub reviews to Neovim. Feature modules call the backend through Connect JSON, a JSON RPC protocol. UI components display records and return user actions. The backend owns durable review data.

The plugin does not access the database or use the web frontend. CodeDiff owns diff buffers and its Git explorer. NuiComponents and NUI own windows and widgets.

## Requirement

### Functional

- FR1: Preserve notes, comparisons, thread actions, reviewed marks, exports, and explicit submission.
- FR2: Keep attribution and permissions in the backend-facing feature modules.
- FR3: Preserve editable text after failed saves and cancellation.
- FR4: Keep the thread pane open during navigation and resolution.
- FR5: Install required plugins through one supported lazy.nvim declaration.

### Non-Functional

- NFR1: Keep the editor responsive during RPCs and provider operations.
- NFR2: Follow the active colorscheme and preserve configured mappings and borders.
- NFR3: Keep UI components independent of session creation, persistence, and forge credentials.

## Design

### Review workspace

The layouts use text captured from Neovim at 100 columns with sample feedback. This example shows the NUI thread pane beside a source buffer. The selected comment uses a background highlight in the editor. The pane stays open when Enter returns focus to code.

```text
  8 -- review implementation                             │ OPEN · 1 reply · src/review.lua:21
  9 -- review implementation                             │──────────────────────────────────────────
 10 -- review implementation                             │ TOP-LEVEL COMMENT · Alex · Published
 11 -- review implementation                             │   Keep the draft after a failed save.
 12 -- review implementation                             │
 13 -- review implementation                             │   └─ REPLY 1 · You · GitHub draft
 14 -- review implementation                             │      Agreed. This now retains the text.
 15 -- review implementation                             │
 16 -- review implementation                             │
 17 -- review implementation                             │
 18 -- review implementation                             │
 19                                                      │
 20 local function save(body)                            │
 21   submit(body, done)                                 │
 22 end                                                  │
                                                         │
```

Git reviews also include CodeDiff's diff buffers and native explorer. Explicit directory reviews use the [directory file pane](ui/files.md). [Search](ui/search.md) and the [composer](ui/composer.md) float over the workspace. Each component page shows its rendered layout in different states.

### Components and Responsibilities

```mermaid
flowchart TD
  Entry["Commands and keymaps"] --> Features["review / notes / sessions / threads / files"]
  Features --> RPC["rpc.lua + transport/connect.lua"]
  RPC --> Backend["LGTM backend"]
  Features --> Views["ui/<br/>Display records and action callbacks"]
  Views --> NC["NuiComponents<br/>Renderer, signals, input, trees"]
  NC --> NUI["NUI<br/>Split, Tree, Line, Text"]
  Views --> NUI
  Features --> Diff["codediff.lua<br/>CodeDiff views and annotation alignment"]
```

| Owner | Responsibility |
| --- | --- |
| `init.lua`, `keymaps.lua` | Commands, mappings, tab hooks, and theme refresh. |
| `context.lua`, `review.lua`, `notes.lua` | Active view state, capabilities, review actions, and saved-file scopes. |
| `threads.lua`, `files.lua` | Comment permissions, source placement, file selection, and reviewed marks. |
| `sessions.lua` | Candidate loading and conversion to display IDs/labels. |
| `ui/` | [Search](ui/search.md), [composer](ui/composer.md), [conversation](ui/conversation.md), [directory files](ui/files.md), and [cards](ui/cards.md). |
| `ui/theme.lua` | Semantic highlight links, configured borders, and dependency diagnostics. |
| `codediff.lua`, `sources.lua`, `anchors.lua` | Diff lifecycle, saved source evidence, placement, and virtual-line alignment. |
| `notifications.lua`, `picker.lua` | nvim-notify messages and the user's simple `vim.ui` prompts. |

### Data and Control Flow

```mermaid
sequenceDiagram
  actor User
  participant Feature as Feature controller
  participant View as UI component
  participant RPC as Backend RPC
  User->>Feature: Command or key
  Feature->>View: Display records and callbacks
  User->>View: Select, edit, or save
  View->>Feature: Stable item ID or body
  Feature->>RPC: Validated request and retry identity
  RPC-->>Feature: Result or error
  Feature-->>View: Updated records or save callback
```

A controller is a feature module that runs application actions. A view is a mounted UI component. Controllers retain review IDs, permissions, and source evidence. Components retain display selection and temporary text. The backend keeps comments, replies, sessions, and receipts.

### Interfaces and Contracts

Global keyboard API uses the configured prefix, which defaults to `<leader>r`. These keys come from `keymaps.lua`. Component-local keys appear in each component's Public API section.

| Prefix + key | Action | Mode |
| --- | --- | --- |
| b / r | Review the branch / working tree. | Normal |
| p / S | Search PRs / resume a session. | Normal |
| D | Review a directory. | Normal |
| n / c | Add a note / line comment. | Normal or Visual |
| f / g | Add a file / general comment. | Normal |
| e / d | Edit / delete the selected comment through its conversation. | Normal |
| t / F | Open all threads / review files. | Normal |
| m | Toggle the current file's reviewed mark. | Normal |
| P | Promote a note. | Normal |
| x | Export the active review, or notes when no review is active. | Normal |
| s / y | Submit the review / sync GitHub drafts. | Normal |
| q | Close the review view. | Normal |


The user API remains `require("lgtm").setup(opts)` and `:Lgtm` commands. See the [plugin guide](../../../apps/lgtm.nvim/README.md) for configuration and commands.

| Boundary | Contract |
| --- | --- |
| UI to controller | Opaque item IDs, file paths, thread/comment IDs, or draft text. |
| Composer to controller | `submit(body, done)`. A nil error acknowledges success. An error retains text for retry. |
| Controller to conversation | Display records, allowed actions, mapping overrides, and readiness callback. |
| Plugin to backend | Identity and capabilities bootstrap, then asynchronous Connect requests through curl. |
| CodeDiff adapter | Preserve source-side locations and align annotations without changing source content. |

UI modules do not call RPCs or decide ownership. The [backend review architecture](../backend/reviews.md) defines persistence, draft synchronization, and submission contracts.

## Implementation

### Package and dependency installation

The root `lazy.lua` declares NuiComponents, NUI, CodeDiff, WhichKey, and nvim-notify. It adds `apps/lgtm.nvim` to the runtime path and applies setup once. Lazy.nvim reads package metadata from the Git root. The root `doc` symlink exposes editor help.

```mermaid
flowchart LR
  Declaration["One LGTM declaration"] --> Lazy["lazy.nvim package loader"]
  Lazy --> Metadata["Root lazy.lua"]
  Metadata --> Dependencies["Managed plugin dependencies"]
  Metadata --> Runtime["apps/lgtm.nvim runtime and setup"]
  Switch["Dev/released source-switch task"] --> Declaration
```

`nvim:install-dev-plugin` selects this checkout root. `nvim:install-released-plugin` selects the repository's main branch. Both preserve user options. Neither task builds the CLI. Restart Neovim after switching.

Other managers and disabled package metadata need explicit dependencies and runtime setup. Older LGTM versions without `lazy.lua` need their version's manual instructions. The CLI, curl, Git, backend GitHub login, and CodeDiff native library remain separate requirements.

### UI and review state

```mermaid
flowchart LR
  Backend["Backend<br/>Sessions, comments, receipts"] --> Controller["Feature controllers<br/>Active review and source locations"]
  Controller -->|"Display records"| Views["UI components<br/>Selection and mounted windows"]
  Controller -->|"draft_id + initial body"| Drafts["Composer draft cache<br/>Cancelled or failed text"]
  Drafts -->|"Reopen the same draft"| Views
  Views -->|"Stable IDs and user actions"| Controller
  Controller -->|"Explicit mutations"| Backend
  Controller -->|"Card rows and source-side mapping"| Adapter["Extmark adapter<br/>CodeDiff alignment"]
```

## References

- [Plugin source](../../../apps/lgtm.nvim/lua/lgtm).
- [Editor journeys](../../../e2e/nvim).
- [Installation and usage](../../../apps/lgtm.nvim/README.md).
- [NuiComponents](https://nui-components.grapp.dev/docs/) and [NUI](https://github.com/MunifTanjim/nui.nvim).
- [lazy.nvim packages](https://lazy.folke.io/packages).
