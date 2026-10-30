describe("When inspecting an installation", function()
  -- fixtures ------------------------------------------------------------------

  local original_health, original_config
  local errors

  before_each(function()
    original_health = vim.health
    original_config = vim.deepcopy(require("lgtm.config").options)
    require("lgtm.config").setup({ endpoint_url = "http://127.0.0.1:1/rpc/" })
    errors = {}
    vim.health = {
      start = function(_) end,
      ok = function(_) end,
      info = function(_) end,
      error = function(message)
        table.insert(errors, message)
      end,
    }
  end)

  after_each(function()
    vim.health = original_health
    require("lgtm.config").options = original_config
  end)

  -- integration tests ---------------------------------------------------------

  describe("When checking an incomplete Neovim installation", function()
    it("Reports missing UI dependencies and an unavailable backend", function()
      -- Act
      require("lgtm.health").check()

      -- Assert
      local messages = table.concat(errors, "\n")
      for _, dependency in ipairs({ "which-key", "nui-components", "nui.split" }) do
        assert.matches("Missing dependency " .. dependency, messages, 1, true)
      end
      assert.matches("Backend unavailable", messages, 1, true)
    end)
  end)
end)
