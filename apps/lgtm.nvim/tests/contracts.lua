local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.runtimepath:prepend(root)
for _, dependency in ipairs({
  "codediff.nvim",
  "which-key.nvim",
  "nvim-notify",
  "nui.nvim",
  "nui-components.nvim",
}) do
  vim.opt.runtimepath:append(root .. "/../../e2e/nvim/.cache/" .. dependency)
end

-- tests -----------------------------------------------------------------------
local function should_load_plugin_shell()
  for _, file in ipairs(vim.fn.glob(root .. "/lua/**/*.lua", false, true)) do
    assert(loadfile(file))
  end
  vim.cmd("runtime plugin/lgtm.lua")
  require("lgtm").setup()
  require("lgtm").setup({})
  assert(vim.g.loaded_lgtm)
  assert(vim.fn.exists(":Lgtm") == 0)
  assert(vim.v.errmsg == "", vim.v.errmsg)
end

should_load_plugin_shell()
