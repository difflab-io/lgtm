local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.runtimepath:prepend(root)
-- tests -----------------------------------------------------------------------
local function should_validate_plugin_contracts()
  for _, file in ipairs(vim.fn.glob(root .. "/lua/**/*.lua", false, true)) do
    assert(loadfile(file))
  end
  assert(loadfile(root .. "/plugin/lgtm.lua"))
  local transport = require("lgtm.transport.connect")
  require("lgtm.config").setup({ endpoint_url = "http://127.0.0.1:1/rpc/" })
  assert(transport.endpoint() == "http://127.0.0.1:1/rpc")
  local states = {
    conversation = {
      comments = { { threadId = "old" }, { threadId = "new" } },
      threads = {
        {
          id = "old",
          anchor = { kind = "line", path = "a", side = "old", start = "2", ["end"] = "4" },
        },
        {
          id = "new",
          anchor = { kind = "line", path = "a", side = "new", start = "2", ["end"] = "4" },
        },
      },
    },
  }
  assert(require("lgtm.threads").at(states, "a", "old", 3)[1].id == "old")
  assert(#require("lgtm.threads").at(states, "a", "new", 5) == 0)
  local health_errors = {}
  local original_health = vim.health
  vim.health = {
    start = function() end,
    ok = function() end,
    info = function() end,
    error = function(message)
      health_errors[#health_errors + 1] = message
    end,
  }
  require("lgtm.health").check()
  vim.health = original_health
  assert(table.concat(health_errors, "\n"):find("Missing dependency which-key", 1, true))
  assert(table.concat(health_errors, "\n"):find("Backend unavailable", 1, true))
  print("Plugin contract checks passed")
end

should_validate_plugin_contracts()
vim.cmd("qa!")
