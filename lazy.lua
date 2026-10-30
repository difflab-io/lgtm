-- lazy.nvim reads package metadata from the repository root.
return {
  {
    "difflab-io/lgtm",
    main = "lgtm",
    lazy = false,
    dependencies = {
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
  },
}
