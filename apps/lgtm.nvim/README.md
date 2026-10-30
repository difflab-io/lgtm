# lgtm.nvim

The Neovim plugin is a shell for a future rewrite. Its previous UI, review workflows and backend transport have been removed. It currently provides a loadable package, an inert setup hook and editor help. There are no LGTM commands or keymaps. Pinned editor dependencies remain in plugin installation and E2E setup for the rewrite.

## Setup

Development checks use Neovim 0.12.5. The shell does not require the CLI or a running backend. The plugin lives in `apps/lgtm.nvim` inside the LGTM monorepo. Add that subdirectory to Neovim's runtime path. The root `doc` symlink exposes editor help.

Retained dependency pins:

| Plugin | Revision |
| --- | --- |
| `grapp-dev/nui-components.nvim` | `1654dd709f13874089eefc80d82e0eb667f7fdfb` |
| `MunifTanjim/nui.nvim` | `10fc361835c856ba4233ef5ea135b919bf3dce97` |
| `mirlge/kak.nvim` (optional) | `b299d2689429b48afe48b1b294a6046487dacb6b` |
| `esmuellert/codediff.nvim` | `09d9ebef2cc5a5c04db7a349cd6c61bdf84ecc8e` |
| `folke/which-key.nvim` | `3aab2147e74890957785941f0c1ad87d0a44c15a` |
| `rcarriga/nvim-notify` | `8701bece920b38ea289b457f902e2ad184131a5d` |

The shell's setup hook does not use these dependencies yet. E2E setup loads them to keep the editor environment ready for the rewrite. CodeDiff also needs its native library; allow its download or follow its source-build instructions.

### lazy.nvim

The repository's root [`lazy.lua`](../../lazy.lua) declares the pinned dependencies and configures the plugin runtime path. Both install commands read the same spec. Keep lazy.nvim package loading enabled with `"lazy"` in `pkg.sources`.

```lua
{
  "difflab-io/lgtm",
  opts = {},
}
```

For development, replace the repository field with `dir = "/absolute/path/to/lgtm"`, pointing to the repository root. The package spec includes Kak as an optional dependency: it preserves the pin when Kak is installed in your configuration, without enabling its keybindings for other users. NuiComponents uses NUI; both dependencies remain available for the rewrite. E2E installation also retains pinned lazy.nvim and Plenary for package-manager fixtures.

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

The [Neovim E2E shell](../../e2e/nvim) retains isolated editor processes, MessagePack control, fixture composition, diagnostics and package installation checks. It uses the shared CLI/backend runtime fixture, so E2E tasks still build the CLI. The required editor profile installs the pinned dependencies and loads CodeDiff, WhichKey, nvim-notify and NuiComponents/NUI; the optional `kak` profile also loads pinned Kak keybindings.

```sh
premise run e2e:nvim
premise run //e2e/nvim:smoke
LGTM_E2E_NVIM_PROFILES=required,kak premise run e2e:nvim
```
