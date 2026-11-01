local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.runtimepath:prepend(root)
vim.opt.runtimepath:prepend(root .. "/tests")
package.path = table.concat({
  root .. "/lua/?.lua",
  root .. "/lua/?/init.lua",
  root .. "/tests/?.lua",
  package.path,
}, ";")

for _, dependency in ipairs({
  "codediff.nvim",
  "which-key.nvim",
  "nvim-notify",
  "nui.nvim",
  "nui-components.nvim",
  "plenary.nvim",
}) do
  local dep_root = root .. "/../../e2e/nvim/.cache/" .. dependency
  vim.opt.runtimepath:append(dep_root)
  package.path = table.concat({
    dep_root .. "/lua/?.lua",
    dep_root .. "/lua/?/init.lua",
    package.path,
  }, ";")
end

-- fixtures --------------------------------------------------------------------

local function load_all_lua_files()
  for _, file in ipairs(vim.fn.glob(root .. "/lua/**/*.lua", false, true)) do
    assert(loadfile(file))
  end
  for _, file in ipairs(vim.fn.glob(root .. "/tests/*_spec.lua", false, true)) do
    assert(loadfile(file))
  end
end

-- tests -----------------------------------------------------------------------

local function should_load_all_plugin_modules_and_pass_headless_unit_specs()
  load_all_lua_files()
  require("models_spec").run()
  require("local_review_service_spec").run()
  require("pr_service_spec").run()

  vim.cmd("runtime plugin/lgtm.lua")
  require("lgtm").setup()
  require("lgtm").setup({})
  assert(vim.g.loaded_lgtm)
  assert(vim.v.errmsg == "", vim.v.errmsg)
end

should_load_all_plugin_modules_and_pass_headless_unit_specs()
