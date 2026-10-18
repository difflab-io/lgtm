local source = debug.getinfo(1, "S").source:sub(2)
local suite = vim.fn.fnamemodify(source, ":p:h")
local root = vim.env.LGTM_REPO_ROOT or vim.fn.fnamemodify(suite, ":h:h")
vim.opt.runtimepath:prepend(root .. "/apps/lgtm.nvim")
for _, dependency in ipairs({ "codediff.nvim", "which-key.nvim", "nvim-notify" }) do
	vim.opt.runtimepath:append(suite .. "/.cache/" .. dependency)
end
vim.o.columns = 120
vim.o.lines = 40
vim.o.swapfile = false
vim.o.shadafile = "NONE"
vim.g.mapleader = " "
require("codediff").setup({})
require("which-key").setup({})
require("notify").setup({ timeout = 5000, stages = "static", render = "minimal", minimum_width = 50, max_width = 50 })
require("lgtm").setup({ identity = { name = "Neovim E2E", email = "nvim-e2e@example.test" } })
vim.cmd("runtime plugin/lgtm.lua")
