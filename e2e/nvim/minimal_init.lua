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
require("codediff").setup({ explorer = { view_mode = "tree" } })
require("which-key").setup({})
if vim.env.LGTM_E2E_KAK == "1" then
	vim.opt.runtimepath:append(suite .. "/.cache/kak.nvim")
	-- Match the user's compatible configuration: full mappings load after WhichKey.
	-- Kak's bundled bridge mutates private WhichKey tables and is not used.
	require("kak.keybinds").setup(vim.deepcopy(require("kak.defaults")))
end
require("notify").setup({ timeout = 5000, stages = "static", render = "minimal", minimum_width = 50, max_width = 50 })
require("lgtm").setup({ identity = { name = "Neovim E2E", email = "nvim-e2e@example.test" } })
vim.cmd("runtime plugin/lgtm.lua")
