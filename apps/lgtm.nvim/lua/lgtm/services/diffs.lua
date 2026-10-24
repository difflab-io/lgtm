-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")

-- Public ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.get_files(state, callback)
  local previous = {}
  for _, file in ipairs(state.files or {}) do
    previous[file.path] = file
  end
  rpc.call(
    "GetDiff",
    { target = state.target, comparison = state.comparison },
    function(err, response)
      if err then
        callback(err)
        return
      end
      state.diff = response.diff
      state.root = response.diff.root
      local files = {}
      local function page(token)
        rpc.call(
          "GetFiles",
          { diff = state.diff, pageToken = token, pageSize = 100 },
          function(error, result)
            if error then
              callback(error)
              return
            end
            for _, file in ipairs(result.files or {}) do
              local saved = previous[file.path]
              files[#files + 1] = saved and saved.identity == file.identity and saved or file
            end
            if result.nextPageToken and result.nextPageToken ~= "" then
              page(result.nextPageToken)
            else
              state.files = files
              callback()
            end
          end
        )
      end
      page(nil)
    end
  )
end

---@param state lgtm.ReviewState
---@param file lgtm.ReviewFile
---@param state lgtm.ReviewState
function M.get_file(state, file, callback)
  return rpc.latest(
    "review-file",
    "GetFile",
    { diff = state.diff, path = file.path },
    function(err, result)
      if not err and require("lgtm.services.state").current == state then
        file.sides = { old = result.oldSide, new = result.newSide }
        file.content = result.newSide and result.newSide.content or ""
        file.baseContent = result.oldSide and result.oldSide.content or ""
        file.contentHash = file.identity
        file.loaded = true
      end
      callback(err)
    end
  )
end

return M
