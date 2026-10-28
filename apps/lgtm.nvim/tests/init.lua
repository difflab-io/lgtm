local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.runtimepath:prepend(root)
vim.opt.runtimepath:prepend(root .. "/../../e2e/nvim/.cache/plenary.nvim")
vim.cmd("runtime plugin/plenary.vim")
