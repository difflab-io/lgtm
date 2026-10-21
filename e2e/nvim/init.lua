-- Runtime setup only. TypeScript owns scenarios, assertions, input and lifecycle.
local root, suite = vim.env.LGTM_REPO_ROOT, vim.env.LGTM_REPO_ROOT .. "/e2e/nvim"
vim.opt.runtimepath:prepend(root .. "/apps/lgtm.nvim")
for _, dependency in ipairs({ "codediff.nvim", "which-key.nvim", "nvim-notify" }) do
  vim.opt.runtimepath:append(suite .. "/.cache/" .. dependency)
end
vim.o.columns, vim.o.lines, vim.o.swapfile, vim.o.shadafile = 120, 40, false, "NONE"
vim.g.mapleader = " "
vim.g.loaded_python3_provider = 0
require("codediff").setup({ explorer = { view_mode = "tree", auto_refresh = false } })
require("which-key").setup({})
if vim.env.LGTM_E2E_KAK == "1" then
  vim.opt.runtimepath:append(suite .. "/.cache/kak.nvim")
  require("kak.keybinds").setup(vim.deepcopy(require("kak.defaults")))
end
require("notify").setup({ timeout = 5000, stages = "static", render = "minimal", minimum_width = 50, max_width = 50 })
require("lgtm").setup({ identity = { name = "Neovim E2E", email = "nvim-e2e@example.test" } })
vim.cmd("runtime plugin/lgtm.lua")
_G.e2e = { choices = {}, inputs = {} }
vim.ui.select = function(items, opts, callback)
  _G.e2e.last_select = { items = items, prompt = opts.prompt }
  local selected = table.remove(_G.e2e.choices, 1) or 1
  local item = type(selected) == "number" and items[selected] or nil
  for _, entry in ipairs(items) do if entry == selected then item = entry end end
  vim.schedule(function() callback(item) end)
end
vim.ui.input = function(opts, callback)
  _G.e2e.last_input = opts
  local value = table.remove(_G.e2e.inputs, 1) or opts.default or ""
  vim.schedule(function() callback(value) end)
end
