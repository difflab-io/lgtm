-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")
local Private = {}

local records = setmetatable({}, { __mode = "k" })

---@return NuiSignalValue
function M.get_data(state)
  if not records[state] then
    require("lgtm.services.threads_service").get_data(state)
    local signal = require("nui-components.signal").create({ value = {} })
    signal.value =
      { files = rawget(state, "files") or {}, reviewed = rawget(state, "reviewed") or {} }
    records[state] = signal
    rawset(state, "files", nil)
    rawset(state, "reviewed", nil)
    local previous = getmetatable(state)
    setmetatable(state, {
      __index = function(target, key)
        if key == "files" or key == "reviewed" then
          return signal:get_value().value[key]
        end
        return previous.__index(target, key)
      end,
      __newindex = function(target, key, value)
        if key == "files" or key == "reviewed" then
          error("File data is owned by files_service")
        end
        previous.__newindex(target, key, value)
      end,
    })
  end
  return records[state].value
end

function M.set_files(state, files)
  local current = M.get_data(state):get_value()
  records[state].value = vim.tbl_extend("force", current, { files = files })
end

function M.set_reviewed(state, reviewed)
  local current = M.get_data(state):get_value()
  records[state].value = vim.tbl_extend("force", current, { reviewed = reviewed })
end

function M.apply_native_files(state, status)
  local previous = {}
  for _, file in ipairs(state.files or {}) do
    if file.group then
      previous[file.group .. ":" .. file.path] = file
    else
      previous[file.path] = file
    end
  end
  local files = {}
  for _, group in ipairs({ "unstaged", "staged" }) do
    for _, entry in ipairs(status[group] or {}) do
      local file = vim.tbl_extend(
        "force",
        {},
        previous[group .. ":" .. entry.path] or previous[entry.path] or {}
      )
      if file.previousPath ~= entry.old_path and state.reviewed then
        M.invalidate_reviewed(state, entry.path)
      end
      file.path, file.status, file.previousPath = entry.path, entry.status, entry.old_path
      file.confirmedRename = entry.old_path ~= nil and entry.status:match("^R") ~= nil
      file.contentHash = file.identity
      file.group = group
      file.native_entry = vim.tbl_extend("force", entry, { group = group })
      files[#files + 1] = file
    end
  end
  M.set_files(state, files)
end

function M.record_content(state, file, result)
  Private.replace_file(
    state,
    vim.tbl_extend("force", {}, file, {
      sides = { old = result.oldSide, new = result.newSide },
      content = result.newSide and result.newSide.content or "",
      baseContent = result.oldSide and result.oldSide.content or "",
      contentHash = file.identity,
      loaded = true,
    })
  )
end

function M.toggle_reviewed(state, file)
  local reviewed = vim.tbl_extend("force", {}, M.get_data(state):get_value().reviewed)
  local previous = reviewed[file.path]
  local selected = not previous and file.contentHash or nil
  reviewed[file.path] = selected
  M.set_reviewed(state, reviewed)
  M.save_reviewed(state, file.path, selected, function(err)
    if err and state.reviewed[file.path] == selected then
      local restored = vim.tbl_extend("force", {}, state.reviewed)
      restored[file.path] = previous
      M.set_reviewed(state, restored)
    end
  end)
end

-- Methods ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.get_reviewed(state)
  M.get_data(state)
  if not state.review or not state.comparison then
    return
  end
  state.reviewed_read = (state.reviewed_read or 0) + 1
  local generation = state.reviewed_read
  state.reviewed_loading = true
  state.reviewed_review = state.review.id
  require("lgtm.services.client").call(
    "GetFileReviews",
    { reviewId = state.review.id },
    function(err, response)
      if state.closed or generation ~= state.reviewed_read then
        return
      end
      state.reviewed_loading = false
      if err then
        state.reviewed_review = nil
        if state.on_error then
          state.on_error(err)
        end
        return
      end
      local reviewed = {}
      for _, file in ipairs(response.files or {}) do
        reviewed[file.path] = file.contentIdentity
      end
      for path, queue in pairs(state.reviewed_pending or {}) do
        reviewed[path] = queue[#queue].identity
      end
      M.set_reviewed(state, reviewed)
      M.validate_reviewed(state)
    end
  )
end

---@param state lgtm.ReviewState
function M.save_reviewed(state, path, identity, done)
  local reload_progress = state.reviewed_loading
  state.reviewed_loading = false
  state.reviewed_read = (state.reviewed_read or 0) + 1
  if not state.review then
    return
  end
  state.reviewed_pending = state.reviewed_pending or {}
  local queue = state.reviewed_pending[path]
  local request = { identity = identity, done = done }
  if queue then
    queue[#queue + 1] = request
    return
  end
  queue = { request }
  state.reviewed_pending[path] = queue
  local function send()
    local current = queue[1]
    require("lgtm.services.client").call("SetFileReviewed", {
      reviewId = state.review.id,
      path = path,
      reviewed = current.identity ~= nil,
      contentIdentity = current.identity,
    }, function(err)
      table.remove(queue, 1)
      if #queue == 0 then
        state.reviewed_pending[path] = nil
        if reload_progress and not state.closed then
          M.get_reviewed(state)
        end
      else
        send()
      end
      if current.done then
        current.done(err)
      end
      if err then
        if state.on_error then
          state.on_error(err)
        end
      end
    end)
  end
  send()
end

---@param state lgtm.ReviewState
function M.invalidate_reviewed(state, path)
  if not state.reviewed or not state.reviewed[path] then
    return
  end
  local reviewed = vim.tbl_extend("force", {}, state.reviewed)
  reviewed[path] = nil
  M.set_reviewed(state, reviewed)
  M.save_reviewed(state, path, nil)
end

---@param state lgtm.ReviewState
function M.validate_reviewed(state)
  for _, file in ipairs(state.files or {}) do
    local identity = state.reviewed and state.reviewed[file.path]
    if
      identity
      and identity ~= "github"
      and file.contentHash
      and not Private.same_content(identity, file.contentHash)
    then
      M.invalidate_reviewed(state, file.path)
    end
  end
end

---@param state lgtm.ReviewState
function M.get_files(state, callback)
  local previous = {}
  for _, file in ipairs(state.files or {}) do
    previous[file.path] = file
  end
  require("lgtm.services.diff_service").get_diff(state, function(err, diff)
    if err then
      callback(err)
      return
    end
    if not state.snapshot then
      state.root = diff.root
    end
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
            M.set_files(state, files)
            callback()
          end
        end
      )
    end
    page(nil)
  end)
end

-- Private ---------------------------------------------------------------------

function Private.replace_file(state, selected)
  M.set_files(
    state,
    vim.tbl_map(function(file)
      if file.path == selected.path and file.group == selected.group then
        return selected
      end
      return file
    end, state.files or {})
  )
end

-- JSON key order can differ across editor processes. Compare source values.
function Private.same_content(first, second)
  if first == second then
    return true
  end
  local first_ok, first_value = pcall(vim.json.decode, first)
  local second_ok, second_value = pcall(vim.json.decode, second)
  return first_ok
    and second_ok
    and type(first_value) == "table"
    and vim.deep_equal(first_value, second_value)
end
return M
