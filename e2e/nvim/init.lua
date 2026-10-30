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
require("notify").setup({
  timeout = 5000,
  stages = "static",
  render = "minimal",
  minimum_width = 50,
  max_width = 50,
})
if vim.env.LGTM_E2E_KAK == "1" then
  vim.opt.runtimepath:append(suite .. "/.cache/kak.nvim")
  require("kak.keybinds").setup(vim.deepcopy(require("kak.defaults")))
end
require("lgtm").setup()
vim.cmd("runtime plugin/lgtm.lua")
