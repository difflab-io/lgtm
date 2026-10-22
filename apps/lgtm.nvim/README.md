# lgtm.nvim

Notes and local/GitHub review through the LGTM backend, with CodeDiff and compact comment overlays. This plugin calls Connect RPCs directly; it never connects to the web frontend or opens the database.

[Setup and troubleshooting](#setup) · [Usage](#usage) · [Editor help](doc/lgtm.txt)

## Setup

Install Neovim **0.11+**, the compatible `lgtm` CLI, and `curl`. Git-based reviews also need Git. GitHub reviews need `gh auth login` for the user running the backend. Notes and local reviews work without GitHub credentials.

Required plugins:

| Plugin | Purpose | Verified revision |
| --- | --- | --- |
| `esmuellert/codediff.nvim` | Diff engine and native library | `09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e` (4.0.6) |
| `folke/which-key.nvim` | Discoverable review mappings | `3aab2147e74890957785941f0c1ad87d0a44c15a` |
| `rcarriga/nvim-notify` | Draft/submission/error notifications | `8701bece920b38ea289b457f902e2ad184131a5d` |

CodeDiff downloads a platform-specific native library; allow the download or follow its source-build instructions. `nui.nvim` is not needed. `lazy.nvim` below is an installation example, not a runtime dependency. While this plugin lives in the LGTM monorepo, use its local directory:

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

Build the CLI from this checkout; older published CLI releases do not include these review RPCs:

```sh
mise trust
mise install
premise run lgtm-cli:build
export PATH="$PWD/target/debug:$PATH"
lgtm up --no-open
```

Start `lgtm up --no-open` before using the plugin. It discovers the daemon from `$LGTM_HOME/server.port` (default `~/.lgtm`). The daemon URL includes `/rpc`; a standalone backend uses its root URL, configured with `endpoint_url` or `LGTM_ENDPOINT_URL`. Run backend and editor as the same local user. For agent attribution configure `identity = { name = "Agent", model = "...", provider = "..." }` instead of email. Attribution and the GitHub posting account are separate.

Run `:checkhealth lgtm` for missing executables, plugins, native library, endpoint and identity setup. Connection errors tell you to start the backend or set its URL. Backend API version 1 with `review_targets`, `feedback_scopes`, `anchor_evidence`, and `diff_service` is required. Build the CLI/backend and plugin from the same checkout when upgrading. Unary Connect calls use asynchronous `vim.system()`/curl; ambiguous writes are never automatically replayed.

## Usage

| Command | Action |
| --- | --- |
| `:Lgtm note`, `:Lgtm notes` | Add a note to the current line/range; inspect notes for the current file |
| `:Lgtm review` | Choose Working, Staged, Branch, or PR |
| `:Lgtm directory [path]` | Review all supported files, including ignored/untracked files |
| `:Lgtm prs [repository URL]` | List open PRs for the current branch and inferred Git repository; optional URL overrides it |
| `:Lgtm sessions [completed]` | Select active sessions or inspect completed sessions |
| `:Lgtm sync` | Sync saved PR drafts or recover a lost draft response without submitting |
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

The backend resolves the selected root and enumerates its content. Nested symlink escapes, binary/oversized files and stale anchors are reported conservatively. Save files before adding notes so the backend can verify their content. Notes use the saved file location and remain separate from a selected review. Existing directory-stored notes remain accessible by that file. Promotion retains original notes and attribution. Cross-scope promotion requires confirming the target relative path; invalid anchors fail without removing source notes.

### Saved-file moves

Saved-file notes and explicit directory reviews follow confirmed moves, including moves between parent directories. LGTM keeps the original source, thread IDs, replies, author, and resolution. A new note records the file's current location. Copies do not move the original discussion.

The plugin observes `BufFilePre`/`BufFilePost` and MiniFiles rename events. Other file managers can report a completed filesystem move:

```lua
vim.api.nvim_exec_autocmds("User", {
  pattern = "LgtmPathRenamed",
  data = { from = "/old/absolute/path", to = "/new/absolute/path" },
})
```

The old path must be absent and the destination must exist. A failed read alone does not prove deletion. LGTM does not scan for renamed files. Refresh retries an unresolved move with its saved operation identity. Lost-line cards retain the original range and excerpt; orphaned discussions remain in `:Lgtm threads` and exports.

### Keys and selection

`<leader>r` shows WhichKey actions with review.nvim's global defaults: `rr` working tree, `rc` comment, `rb` branch, `rn` note, `re` edit, `rd` delete, and `rx` export. New actions use `rD` directory, `rf` file comment, `rg` general comment, `rt` threads, `rF` files, `rp` PRs, `rS` sessions, `rP` promote, `rs` submits the active review and `rq` closes its view. No mapping is installed on the prefix itself. Visual selection works for note/comment ranges.

PR and local-session selection use a built-in searchable picker. Type to filter, use Up/Down (or Ctrl-N/Ctrl-P) to move, Enter to select and Esc to cancel. No additional fuzzy-picker plugin is needed; an empty result keeps the search open.

Git reviews use CodeDiff's native file tree and Git revision buffers. The first local diff opens while PR comments load. If base/head commits exist in the checkout, the backend loads only metadata and comments. Otherwise, it loads only the selected remote file. Snapshot buffers do not start language servers.

If PR loading fails, LGTM shows the cause and keeps the diff open. After you fix the cause, run `:Lgtm refresh` to retry. Commenting, submission and reconciliation stay blocked until the review loads.

The PR identifies your review. Reopening, refreshing, or saving a PR draft detects new base or head commits and updates the current comparison. While an attempt is active, LGTM keeps the same review ID and pending GitHub review. Unsent selections follow a unique text match; lost or ambiguous selections become file-level feedback with the original location and excerpt. Existing remote comments use GitHub’s current locations. No refresh submits or discards a review. Review the displayed comparison before explicit submission; a push that races submission requires another refresh.

Branch review compares the common ancestor of a base branch with the current HEAD. Working compares HEAD with saved working files. Staged compares HEAD with the Git index. A clean comparison reports that it has no changes.

Ignored and non-Git directory reviews use the LGTM file pane. Enter selects a file, `f` toggles the pane, `]f`/`[f` visit files, and Tab/Shift-Tab visit unreviewed files. Toggle the current file's reviewed (seen) mark with `<leader>rm`, or press `m` on a file in either review tree. A ✓ checkmark identifies reviewed files in PR, branch, working-tree, staged, and directory reviews. A content change clears the mark. Local marks persist across editor restarts. PR marks use GitHub's Viewed state. Notes have no reviewed-file marks. Kak is optional and is covered by a separate E2E profile.

Comments use full-width bordered cards with draft/resolved status, reply count, author, and a one-line text preview. File comments appear above the file. Enter/K opens the centered thread modal. At an annotated line:

| Key | Action |
| --- | --- |
| `Enter` / `K` | Open threads at this side/line |
| `Tab` / `Shift-Tab` in popup | Select next/previous root comment or reply |
| `c` in popup | Reply to selected thread |
| `e` in popup | Edit selected owned comment |
| `dd` in popup | Confirm deletion of selected owned comment |
| `dd` at line | Open thread selection before deleting |
| `x` | Resolve/reopen selected thread |
| `q` / `Esc` | Close popup and restore source pane/cursor |
| `]c` / `[c`, `]n` / `[n` | Navigate annotations |
| `za` / `zA` | Expand/collapse at cursor / across file |
| `gH` | Toggle overlays |

File/general threads appear in `:Lgtm threads`; Enter navigates to their file. Inline layout places deleted old-side text in CodeDiff virtual lines; use the thread pane or switch to side-by-side to create/navigate old-side line comments. Composers save with `Ctrl-S`. Failed saves keep the text in the composer. Closing with `q`/Esc hides the draft buffer; it is not sent. A notification confirms a save only after the backend acknowledges it. PR line/file comments and replies sync to a private pending review on GitHub as you save. The notification says “Draft saved on GitHub” only after GitHub confirms it. A failed sync keeps the saved local draft and reports the failure. Use `:Lgtm sync` (`<leader>ry`) to sync existing local drafts or recover uncertain saves; accepted comments are never blindly reposted. Notes and local reviews stay local. General PR comments are posted only at review submission. GitHub has no private standalone general-comment draft. Closing or opening a review never uploads existing local drafts automatically.

A successful remote submission is announced only after confirmation; uncertain results retain the draft and offer `:Lgtm reconcile`. Pending/sending/reconciling/failed results cannot start a new submission through the UI; use explicit reconciliation. Reconciliation may finish an existing pending review only when every receipt is verified. Missing or ambiguous receipts require inspecting/completing or discarding the pending review on GitHub; the plugin never blindly reposts comments. Other users' comments cannot be edited/deleted. Posted remote comment actions require provider support and surface backend permission errors.

## Appearance

LGTM follows your active colorscheme through semantic Neovim highlight links. The same styling applies to pickers, composers, thread modals, comment cards, and LGTM notifications. No fixed accent color is required. Notification accents come from `DiagnosticInfo`, `DiagnosticWarn`, or `DiagnosticError`.

Add overrides to your existing setup options:

```lua
ui = {
  border = "rounded", -- single, double, or a Neovim border specification
  highlights = {
    normal = "NormalFloat",
    border = "FloatBorder",
    title = "Title",
    muted = "Comment",
    selection = "Visual",
    info = "DiagnosticInfo", -- use "Special" for your theme's alternative accent
    success = "DiagnosticOk",
    warning = "DiagnosticWarn",
    error = "DiagnosticError",
  },
}
```

You can also override `LgtmNormal`, `LgtmBorder`, `LgtmTitle`, `LgtmMuted`, `LgtmSelection`, `LgtmInfo`, `LgtmSuccess`, `LgtmWarning`, or `LgtmError` with `vim.api.nvim_set_hl()` after setup. Links follow colorscheme changes. Choice and input prompts use your configured `vim.ui` provider.

The thread modal separates its status banner from the conversation. Replies are indented and numbered below the **Top-level comment**. The selected comment block is highlighted; Tab/Shift-Tab changes the selected comment. Edit titles and deletion prompts identify the selected root or reply. The modal omits the filename already visible in the diff; the all-files thread pane retains locations.

## Development

Plugin checks live in this package's `mise.toml`. Complete editor workflows live in [`e2e/nvim`](../../e2e/nvim), with their own `mise.toml`, exact dependency pins, isolated child editors and real backend/database. Invoke them through Premise's registered project tasks. Run suite `install` once before `test`. The harness creates only local temporary Git repositories, downloads exact upstream plugins, and cleans up the backend and editor state. Live GitHub tests, when configured separately, must use verified private disposable repositories.

After submitting a PR review, you can keep using the same diff. The next comment or reply starts a new pending review for that PR while preserving earlier published feedback. Opening or cancelling a composer alone does not create another review.
