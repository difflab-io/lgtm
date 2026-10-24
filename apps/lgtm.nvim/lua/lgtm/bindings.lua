-- Buffer mappings belong to a review or notes view. Preserve prior CodeDiff/Kak/user mappings.
-- State -----------------------------------------------------------------------

local M = {}

-- Public ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.set(state, modes, key, callback, options)
  local buf = options.buffer
  state.bindings = state.bindings or {}
  state.bindings[buf] = state.bindings[buf] or {}
  for _, mode in ipairs(type(modes) == "table" and modes or { modes }) do
    local id = mode .. ":" .. key
    local binding = state.bindings[buf][id]
    if not binding then
      local previous = vim.api.nvim_buf_call(buf, function()
        return vim.fn.maparg(key, mode, false, true)
      end)
      binding = { mode = mode, key = key, previous = previous }
      state.bindings[buf][id] = binding
    end
    vim.keymap.set(mode, key, callback, options)
    binding.installed = vim.api.nvim_buf_call(buf, function()
      return vim.fn.maparg(key, mode, false, true).callback
    end)
  end
end

---@param state lgtm.ReviewState
function M.restore(state)
  for buf, mappings in pairs(state.bindings or {}) do
    if vim.api.nvim_buf_is_valid(buf) then
      vim.api.nvim_buf_call(buf, function()
        for _, binding in pairs(mappings) do
          local current = vim.fn.maparg(binding.key, binding.mode, false, true)
          if current.buffer == 1 and current.callback == binding.installed then
            vim.keymap.del(binding.mode, binding.key, { buffer = buf })
            if binding.previous.buffer == 1 then
              vim.fn.mapset(binding.mode, false, binding.previous)
            end
          end
        end
      end)
    end
  end
  state.bindings = nil
end

return M
