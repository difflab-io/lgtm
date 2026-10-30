# lgtm.nvim

The Neovim plugin is a shell for a future rewrite. Its previous UI, review workflows and backend transport have been removed. It currently provides a loadable package, an inert setup hook and editor help. There are no LGTM commands, keymaps or runtime plugin dependencies.

## Setup

Development checks use Neovim 0.12.5. The shell does not require the CLI or a running backend. The plugin lives in `apps/lgtm.nvim` inside the LGTM monorepo. Add that subdirectory to Neovim's runtime path. The root `doc` symlink exposes editor help.

### lazy.nvim

```lua
{
  "difflab-io/lgtm",
  lazy = false,
  config = function(plugin)
    vim.opt.runtimepath:append(plugin.dir .. "/apps/lgtm.nvim")
    require("lgtm").setup()
    vim.cmd("runtime plugin/lgtm.lua")
  end,
}
```

For development, replace the repository field with `dir = "/absolute/path/to/lgtm"`. If `dir` points directly to `apps/lgtm.nvim`, use `main = "lgtm", opts = {}` and omit the callback instead.

### Native Git packages

Clone LGTM into a native package directory:

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

## Setup contract

`require("lgtm").setup()` and `setup({})` do nothing. Previous feature options no longer have an effect. The `:Lgtm` command and `:checkhealth lgtm` integration are unavailable while the plugin is rewritten.

## Development

Use `premise run nvim:install-dev-plugin` to point your editor at this worktree, or `premise run nvim:install-released-plugin` to switch to `origin/main`. Restart Neovim after switching. These tasks modify your local lazy.nvim spec.

Plugin load checks remain in this package's `mise.toml`:

```sh
premise run nvim:test
premise run nvim:lint
premise run nvim:format:check
```

The [Neovim E2E shell](../../e2e/nvim) retains isolated editor processes, MessagePack control, fixture composition, diagnostics and package installation checks. It uses the shared CLI/backend runtime fixture, so E2E tasks still build the CLI. The default editor profile has no external plugin dependencies; the optional `kak` profile uses its pinned dependency.

```sh
premise run e2e:nvim
premise run //e2e/nvim:smoke
LGTM_E2E_NVIM_PROFILES=required,kak premise run e2e:nvim
```
