-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")

local records = setmetatable({}, { __mode = "k" })

-- Methods ---------------------------------------------------------------------

---@return NuiSignalValue
function M.get_data(state)
  if not records[state] then
    require("lgtm.services.threads_service").get_data(state)
    local signal = require("nui-components.signal").create({ value = {} })
    signal.value = { diff = rawget(state, "diff"), sources = {} }
    records[state] = signal
    rawset(state, "diff", nil)
    local previous = getmetatable(state)
    setmetatable(state, {
      __index = function(target, key)
        if key == "diff" then
          return signal:get_value().value.diff
        end
        return previous.__index(target, key)
      end,
      __newindex = function(target, key, value)
        if key == "diff" then
          error("Diff data is owned by diff_service")
        end
        previous.__newindex(target, key, value)
      end,
    })
  end
  return records[state].value
end

function M.clear(state)
  M.get_data(state)
  records[state].value = { sources = {} }
end

function M.get_diff(state, callback)
  rpc.call("GetDiff", {
    target = state.target,
    comparison = state.comparison,
    reviewId = state.review and state.review.id,
    revisionId = state.snapshot and state.snapshot.id,
  }, function(err, response)
    if not err then
      local current = M.get_data(state):get_value()
      records[state].value = vim.tbl_extend("force", current, { diff = response.diff })
    end
    callback(err, response and response.diff)
  end)
end

---@param state lgtm.ReviewState
---@param file lgtm.ReviewFile
function M.get_file(state, file, callback)
  return rpc.latest(
    "review-file",
    "GetFile",
    { diff = state.diff, path = file.path },
    function(err, result)
      if not err and require("lgtm.services.state").current == state then
        require("lgtm.services.files_service").record_content(state, file, result)
      end
      callback(err)
    end
  )
end
return M
