vim.opt.runtimepath:prepend(vim.env.LGTM_TEST_LAZY_DIR)
vim.o.columns, vim.o.lines = 120, 40
vim.o.swapfile, vim.o.shadafile = false, "NONE"

-- Keep a valid existing cache that has never discovered LGTM package metadata.
local cache = vim.fn.stdpath("state") .. "/lazy/pkg-cache.lua"
if vim.fn.filereadable(cache) == 0 then
  vim.fn.mkdir(vim.fn.fnamemodify(cache, ":h"), "p")
  vim.fn.writefile({ "return {pkgs={},version=" .. require("lazy.pkg").VERSION .. "}" }, cache)
end

require("lazy").setup({
  { import = "plugins" },
  {
    "folke/which-key.nvim",
    opts = {
      delay = 17,
    },
  },
  {
    "rcarriga/nvim-notify",
    opts = {
      timeout = 4321,
      stages = "static",
    },
  },
  {
    "esmuellert/codediff.nvim",
    opts = {
      explorer = {
        auto_refresh = false,
      },
    },
  },
}, {
  rocks = {
    enabled = false,
  },
  checker = {
    enabled = false,
  },
  change_detection = {
    enabled = false,
  },
  install = {
    colorscheme = { "default" },
  },
})
