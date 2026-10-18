# lgtm.nvim

Notes and local/GitHub review through the LGTM backend, with CodeDiff and compact
comment overlays. This plugin calls Connect RPCs directly; it never connects to
the web frontend or opens the database.

[Setup and troubleshooting](#setup) · [Usage](#usage) · [Editor help](doc/lgtm.txt)

## Setup

Install Neovim **0.11+**, the compatible `lgtm` CLI, and `curl`. Git-based reviews
also need Git. GitHub reviews need `gh auth login` for the user running the
backend. Notes and local reviews work without GitHub credentials.

Required plugins:

| Plugin | Purpose | Verified revision |
| --- | --- | --- |
| `esmuellert/codediff.nvim` | Diff engine and native library | `09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e` (4.0.6) |
| `folke/which-key.nvim` | Discoverable review mappings | `3aab2147e74890957785941f0c1ad87d0a44c15a` |
| `rcarriga/nvim-notify` | Draft/submission/error notifications | `8701bece920b38ea289b457f902e2ad184131a5d` |

CodeDiff downloads a platform-specific native library; allow the download or
follow its source-build instructions. `nui.nvim` is not needed. `lazy.nvim` below
is an installation example, not a runtime dependency. While this plugin lives in
the LGTM monorepo, use its local directory:

```lua
{
  dir = "/absolute/path/to/lgtm/apps/lgtm.nvim",
  dependencies = {
    { "esmuellert/codediff.nvim", commit = "09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e", opts = {} },
    { "folke/which-key.nvim", commit = "3aab2147e74890957785941f0c1ad87d0a44c15a", opts = {} },
    { "rcarriga/nvim-notify", commit = "8701bece920b38ea289b457f902e2ad184131a5d", opts = {} },
  },
  opts = {
    identity = { name = "Your Name", email = "you@example.com" },
    -- endpoint_url = "http://127.0.0.1:PORT/rpc", -- optional daemon override
    prefix = "<leader>r",
    -- mappings = { ["K"] = "<leader>rk" }, -- thread map overrides
  },
}
```

Build the CLI from this checkout; older published CLI releases do not include
these review RPCs:

```sh
mise trust
mise install
premise run lgtm-cli:build
export PATH="$PWD/target/debug:$PATH"
lgtm up --no-open
```

Start `lgtm up --no-open` before using the plugin. It discovers the daemon from
`$LGTM_HOME/server.port` (default `~/.lgtm`). The daemon URL includes `/rpc`;
a standalone backend uses its root URL, configured with `endpoint_url` or
`LGTM_ENDPOINT_URL`. Run backend and editor as the same local user. For agent
attribution configure `identity = { name = "Agent", model = "...", provider =
"..." }` instead of email. Attribution and the GitHub posting account are separate.

Run `:checkhealth lgtm` for missing executables, plugins, native library, endpoint
and identity setup. Connection errors tell you to start the backend or set its
URL. Backend API version 1 is required. Unary Connect calls use asynchronous
`vim.system()`/curl; ambiguous writes are never automatically replayed.

## Usage

| Command | Action |
| --- | --- |
| `:Lgtm note`, `:Lgtm notes` | Add a note to the current line/range; inspect notes for the file's directory |
| `:Lgtm review` | Choose Working, Staged, or PR |
| `:Lgtm directory [path]` | Review all supported files, including ignored/untracked files |
| `:Lgtm prs [repository URL]` | List open PRs for the current branch and inferred Git repository; optional URL overrides it |
| `:Lgtm sessions [completed]` | Select active sessions or inspect completed sessions |
| `:Lgtm refresh` | Refresh content/threads; retain drafts on failure |
| `:Lgtm layout` | Toggle CodeDiff side-by-side / inline layout |
| `:Lgtm files`, `:Lgtm threads` | Focus the persistent file pane or open cross-file threads |
| `:Lgtm comment [line/file/general]` | Compose a draft with the appropriate anchor |
| `:Lgtm promote` | Pick source note, preview target path/anchor, promote into selected review |
| `:Lgtm export notes`, `:Lgtm export review` | Preview Markdown, save a file, or copy to clipboard |
| `:Lgtm save` | Confirm durable session state; mutations persist immediately |
| `:Lgtm complete` | Complete a local review |
| `:Lgtm submit` | Choose comment/approve/request-changes and an optional final body |
| `:Lgtm reconcile` | Check an uncertain submission before retrying |
| `:Lgtm close` | Close the review view; keep its durable session/drafts |

For an ignored or symlinked plan directory:

```vim
:Lgtm directory .difflab/plans/specific-plan
```

The backend resolves the selected root and enumerates its content. Nested symlink
escapes, binary/oversized files and stale anchors are reported conservatively.
Save files before adding notes so the backend can verify their content. Notes
are attached to their containing directory and remain separate from a selected
review. Promotion retains original notes and attribution. Cross-scope promotion
requires confirming the target relative path; invalid anchors fail without
removing source notes.

`<leader>r` shows WhichKey actions with review.nvim's global defaults: `rr`
working tree, `rc` commits, `rb` branch, `rn` note, `re` edit, `rd` delete, and
`rx` export. New actions use `rC` draft comment, `rD` directory, `rf` file
comment, `rg` general comment, `rt` threads, `rF` files, `rp` PRs, `rs` sessions,
`rP` promote, `rS` submit and `rq` close. No mapping is installed on the prefix
itself. Visual selection works for note/comment ranges.

PR and local-session selection use a built-in searchable picker. Type to filter,
use Up/Down (or Ctrl-N/Ctrl-P) to move, Enter to select and Esc to cancel. No
additional fuzzy-picker plugin is needed; an empty result keeps the search open.

Review views keep a file pane beside CodeDiff, including for arbitrary ignored
or non-Git directories. It shows the selected file, changed content (`M`) and
reviewed marks (`✓`). Enter selects a file; `f` toggles the pane, `]f`/`[f` visit
all files, and Tab/Shift-Tab visit unreviewed files. Mark the current file with
`<leader>rm` or `m` in the file pane. Refreshing changed content clears its reviewed
mark. These navigation maps apply only inside review panes.

Comments start as collapsed line markers showing draft/resolved status and
thread/comment counts. At an annotated line:

| Key | Action |
| --- | --- |
| `Enter` / `K` | Open threads at this side/line |
| `c` in popup | Reply to selected thread |
| `e` in popup | Edit selected owned comment |
| `dd` in popup | Confirm deletion of selected owned comment |
| `dd` at line | Open thread selection before deleting |
| `x` | Resolve/reopen selected thread |
| `q` / `Esc` | Close popup and restore source pane/cursor |
| `]c` / `[c`, `]n` / `[n` | Navigate annotations |
| `za` / `zA` | Expand/collapse at cursor / across file |
| `gH` | Toggle overlays |

File/general threads appear in `:Lgtm threads`; Enter navigates to their file.
Inline layout places deleted old-side text in CodeDiff virtual lines; use the
thread pane or switch to side-by-side to create/navigate old-side line comments.
Composers save with `Ctrl-S`. Failed saves keep the text in the composer. Closing
with `q`/Esc hides the draft buffer; it is not sent. A notification confirms a
save only after the backend acknowledges it. A successful remote submission is
announced only after confirmation; uncertain results retain the draft and offer
`:Lgtm reconcile`. Pending/sending/reconciling/failed results cannot start a new
submission through the UI; use explicit reconciliation. Reconciliation may finish an existing pending review only when every receipt is verified. Missing or ambiguous receipts require inspecting/completing or discarding the pending review on GitHub; the plugin never blindly reposts comments. Other users' comments cannot be edited/deleted. Posted remote
comment actions require provider support and surface backend permission errors.

## Development

Plugin checks live in this package's `mise.toml`. Complete editor workflows live
in [`e2e/nvim`](../../e2e/nvim), with their own `mise.toml`, exact dependency pins,
isolated child editors and real backend/database. Invoke them through Premise's
registered project tasks. Run suite `install` once before `test`. The harness
creates only local temporary Git repositories, downloads exact upstream plugins,
and cleans up the backend and editor state. Live GitHub tests, when configured
separately, must use verified private disposable repositories.
