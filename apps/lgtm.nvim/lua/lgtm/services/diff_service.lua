-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")
local anchors = require("lgtm.models.source")
local source_kinds = { commit = 1, index_file = 2, working_file = 3, local_file = 4 }

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

function M.get_saved_stamp(stat)
  if not stat or stat.type ~= "file" then
    return false
  end
  return { size = stat.size, mtime = stat.mtime, ctime = stat.ctime, ino = stat.ino, dev = stat.dev }
end

---@param input { root: string, path: string, kind: 'commit'|'index_file'|'working_file'|'local_file', commit: string? }
function M.get_source(input, callback)
  local request = vim.tbl_extend("force", input, { kind = source_kinds[input.kind] })
  rpc.call("GetSource", request, function(err, response)
    local selected = response and response.selected
    if err or not selected then
      callback(err, {})
      return
    end
    local result = {
      content = selected.content or "",
      source = anchors.source_from_wire(selected.source),
      selection = { saved = request },
    }
    if input.kind == "working_file" or input.kind == "local_file" then
      vim.uv.fs_stat(vim.fs.joinpath(input.root, input.path), function(_, stat)
        vim.schedule(function()
          result.saved_stamp = M.get_saved_stamp(stat)
          callback(nil, result)
        end)
      end)
    else
      callback(nil, result)
    end
  end)
end

---@param state lgtm.ReviewState
function M.get_selected_source(state, file, side, callback)
  local function selected(err, source)
    if not err then
      local current = M.get_data(state):get_value()
      local sources = vim.tbl_extend("force", {}, current.sources)
      sources[side] = source
      records[state].value = vim.tbl_extend("force", current, { sources = sources })
    end
    callback(err, source)
  end
  if not state.git_root and file.sides then
    local content = file.sides[side]
    selected(nil, content and {
      content = content.content or "",
      source = anchors.source_from_wire(content.source),
      selection = { file = { diff = state.diff, path = file.path } },
    } or {})
    return
  end
  local kind = not state.git_root and "local_file"
    or side == "old" and "commit"
    or state.mode == "working" and "working_file"
    or state.mode == "staged" and "index_file"
    or "commit"
  local root = state.git_root or state.root
  if not root then
    selected({ message = "Selected source root is unavailable" }, {})
    return
  end
  M.get_source({
    root = root,
    path = side == "old" and (file.previousPath or file.old_path or file.path) or file.path,
    kind = kind,
    commit = kind == "commit" and (side == "old" and state.base or state.head) or nil,
  }, selected)
end

---@param state lgtm.ReviewState
function M.get_diff(state, callback)
  rpc.call(
    "GetDiff",
    { target = state.target, comparison = state.comparison },
    function(err, response)
      if not err then
        local current = M.get_data(state):get_value()
        records[state].value = vim.tbl_extend("force", current, { diff = response.diff })
      end
      callback(err, response and response.diff)
    end
  )
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
