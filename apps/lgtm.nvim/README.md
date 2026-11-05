# lgtm.nvim

`lgtm.nvim` provides reactive Neovim UI scaffolding for **Local Review** (always on during normal buffer editing) and **PR Review** (activated after selecting an open branch pull request into a `CodeDiff` session). It is built on `nui-components.nvim`, `nui.nvim`, `codediff.nvim`, `which-key.nvim`, and `nvim-notify`.

## Setup

Development checks use Neovim 0.12.5. The UI scaffolding runs against reactive in-memory services (`LocalReviewService` and `PrService`) and does not require a running backend daemon. The plugin lives in `apps/lgtm.nvim` inside the LGTM monorepo. Add that subdirectory to Neovim's runtime path. The root `doc` symlink exposes editor help.

Retained dependency pins:

| Plugin | Revision |
| --- | --- |
| `grapp-dev/nui-components.nvim` | `1654dd709f13874089eefc80d82e0eb667f7fdfb` |
| `MunifTanjim/nui.nvim` | `10fc361835c856ba4233ef5ea135b919bf3dce97` |
| `mirlge/kak.nvim` (optional) | `b299d2689429b48afe48b1b294a6046487dacb6b` |
| `esmuellert/codediff.nvim` | `09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e` |
| `folke/which-key.nvim` | `3aab2147e74890957785941f0c1ad87d0a44c15a` |
| `rcarriga/nvim-notify` | `8701bece920b38ea289b457f902e2ad184131a5d` |

The setup hook requires CodeDiff, WhichKey, nvim-notify and NuiComponents/NUI. It also loads Kak when available, without configuring its keybindings. CodeDiff also needs its native library; allow its download or follow its source-build instructions.

### lazy.nvim

Keep the dependency declarations in your plugin spec:

```lua
{
  "difflab-io/lgtm",
  lazy = false,
  dependencies = {
    { "mirlge/kak.nvim", commit = "b299d2689429b48afe48b1b294a6046487dacb6b", optional = true },
    {
      "grapp-dev/nui-components.nvim",
      commit = "1654dd709f13874089eefc80d82e0eb667f7fdfb",
      dependencies = {
        { "MunifTanjim/nui.nvim", commit = "10fc361835c856ba4233ef5ea135b919bf3dce97" },
      },
    },
    {
      "esmuellert/codediff.nvim",
      commit = "09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e",
      opts = {},
    },
    {
      "folke/which-key.nvim",
      commit = "3aab2147e74890957785941f0c1ad87d0a44c15a",
      opts = {},
    },
    { "rcarriga/nvim-notify", commit = "8701bece920b38ea289b457f902e2ad184131a5d", opts = {} },
  },
  config = function(plugin, opts)
    vim.opt.runtimepath:append(plugin.dir .. "/apps/lgtm.nvim")
    require("lgtm").setup(opts)
    vim.cmd("runtime plugin/lgtm.lua")
  end,
}
```

For development, replace the repository field with `dir = "/absolute/path/to/lgtm"`. If `dir` points directly to `apps/lgtm.nvim`, retain the dependencies, use `main = "lgtm", opts = {}` and omit the callback. Kak remains optional and uses your existing keybinding configuration.

### Native Git packages

Install the required dependencies listed above, then clone LGTM into a native package directory:

```sh
git clone --depth 1 https://github.com/difflab-io/lgtm.git ~/.local/share/nvim/site/pack/plugins/start/lgtm
```

Add this to your Neovim configuration:

```lua
local root = vim.fn.stdpath("data") .. "/site/pack/plugins/start/lgtm"
vim.opt.runtimepath:append(root .. "/apps/lgtm.nvim")
require("lgtm").setup()
vim.cmd("runtime plugin/lgtm.lua")
```

Change `root` for a custom package directory. Run `:helptags ALL` after manual installation, then use `:help lgtm.txt`.

## Review Modes and UI Components

`lgtm.nvim` supports two review modes:

- **Local Review (Always On):** Active during normal buffer editing (`LocalReviewService`). Create or reply to local review threads at the cursor line (`<leader>rc`), view inline `anchored-thread-summary` virtual-line overlays, press `<CR>` or `<leader>ro` on an anchored line to open `threads-pane` directly in `thread-detail-view`, or toggle the right-hand `threads-pane` (`<leader>rt`) to browse, fold (`za` / `zA`), reply (`c` / `r`), edit (`e`), delete (`dd`), and resolve (`x`) local threads.
- **PR Review (`CodeDiff`):** Open `pr-select-modal` (`<leader>rp` or `:Lgtm pr`) to browse open pull requests for the current branch on the left and preview the Markdown PR description on the right. Selecting a PR launches a `CodeDiff` session with the file tree on the left and diff view on the right, scopes inline overlays and `threads-pane` to PR threads (`PrService`), and enables overall PR review submission (`Comment`, `Approve`, `Request Changes`) via `pr-submit-editor` (`<leader>rs`, `<leader>ra`, `<leader>rr`).

## Keymaps and Commands

Global mappings use the `<leader>r` prefix in `which-key` so they never collide with LazyVim's `<leader>l` menu:

| Keymap / Command | Mode | Description |
| --- | --- | --- |
| `<leader>rc` / `:Lgtm comment` | Normal, Visual | Open `comment-editor` to draft a new thread or reply at the cursor line (`c` in PR diff buffers and `threads-pane`) |
| `<leader>rp` / `:Lgtm pr` | Normal | Open `pr-select-modal` to pick an open branch PR and launch `CodeDiff` |
| `<leader>rt` / `:Lgtm threads` | Normal | Toggle the right-hand `threads-pane` split |
| `<leader>ro` / `<CR>` / `:Lgtm thread` | Normal | Open `threads-pane` directly in `thread-detail-view` for the thread at the cursor |
| `<leader>rx` / `x` / `:Lgtm resolve` | Normal | Toggle resolved state for the thread at the cursor |
| `<leader>rs` / `:Lgtm submit` | Normal | Open `pr-submit-editor` for the active PR (`Comment` default) |
| `<leader>ra` / `:Lgtm approve` | Normal | Open `pr-submit-editor` pre-selected to `Approve` |
| `<leader>rr` / `:Lgtm request_changes` | Normal | Open `pr-submit-editor` pre-selected to `Request Changes` |
| `]c` / `[c` | Normal | Jump to next / previous anchored thread in the current buffer |
| `za` / `zA` | Normal | Toggle fold on the current thread / fold or unfold all threads |
| `<BS>` / `h` | `threads-pane` (detail) | Return from `thread-detail-view` to `threads-summary-view` |

## Setup contract

`require("lgtm").setup()` and `setup({})` load the retained dependencies, initialize `LocalReviewService`, `PrService`, and `ThreadsOverlay`, and register `:Lgtm` and `<leader>r` WhichKey bindings without opening any windows on startup.

## Development

The development and release installation scripts remain available. Both preserve the dependency pins in the generated lazy.nvim spec:

```sh
# Point Neovim at this worktree:
premise run nvim:install-dev-plugin
# Switch to the version on origin/main:
premise run nvim:install-released-plugin
```

Restart Neovim after switching. These tasks modify your local lazy.nvim spec.

Plugin load checks remain in this package's `mise.toml`:

```sh
premise run nvim:test
premise run nvim:lint
premise run nvim:format:check
```

The [Neovim E2E shell](../../e2e/nvim) runs isolated editor processes, MessagePack control, fixture composition, diagnostics, package installation checks, and end-to-end specs for Local Review (`local-review.spec.ts`) and PR Review (`pr-review.spec.ts`). The required editor profile installs the pinned dependencies and loads CodeDiff, WhichKey, nvim-notify and NuiComponents/NUI; the optional `kak` profile also loads pinned Kak keybindings.

```sh
premise run e2e:nvim
premise run //e2e/nvim:smoke
LGTM_E2E_NVIM_PROFILES=required,kak premise run e2e:nvim
```
