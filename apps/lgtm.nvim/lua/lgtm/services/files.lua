-- State -----------------------------------------------------------------------
local M = {}

-- Public ----------------------------------------------------------------------
---@param state lgtm.ReviewState
function M.get_reviewed(state)
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
      state.reviewed = {}
      for _, file in ipairs(response.files or {}) do
        state.reviewed[file.path] = file.contentIdentity
      end
      for path, queue in pairs(state.reviewed_pending or {}) do
        state.reviewed[path] = queue[#queue].identity
      end
      if state.on_files_changed then
        state.on_files_changed()
      end
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
  state.reviewed[path] = nil
  M.save_reviewed(state, path, nil)
end

---@param state lgtm.ReviewState
function M.validate_reviewed(state)
  if not state.git_root or not state.reviewed then
    return
  end
  local selected = {}
  for _, file in ipairs(state.files or {}) do
    if state.reviewed[file.path] then
      selected[#selected + 1] = file
    end
  end
  if #selected == 0 then
    return
  end
  state.reviewed_generation = (state.reviewed_generation or 0) + 1
  local generation = state.reviewed_generation
  local function invalidate(file, changed)
    if state.closed or state.reviewed_generation ~= generation then
      return
    end
    if changed then
      M.invalidate_reviewed(state, file.path)
      if state.on_files_changed then
        state.on_files_changed()
      end
    end
  end
  if state.mode == "working" then
    for _, file in ipairs(selected) do
      local expected = file.saved_stamp
      if expected == nil then
        local valid, identity = pcall(vim.json.decode, state.reviewed[file.path])
        require("lgtm.services.sources").file(
          state.git_root,
          file.path,
          "working_file",
          function(err, source)
            invalidate(
              file,
              err ~= nil
                or not valid
                or type(identity) ~= "table"
                or not source.source
                or not identity.new
                or source.source.content_hash ~= identity.new.content_hash
            )
            if not err and source then
              file.saved_stamp = source.saved_stamp
            end
          end
        )
      else
        vim.uv.fs_stat(vim.fs.joinpath(state.git_root, file.path), function(err, stat)
          vim.schedule(function()
            local absent = expected == false and err and err:match("ENOENT")
            invalidate(
              file,
              not absent
                and (
                  err ~= nil
                  or not vim.deep_equal(
                    expected,
                    require("lgtm.services.sources").saved_stamp(stat)
                  )
                )
            )
          end)
        end)
      end
    end
  elseif state.mode == "staged" then
    -- Bound each command even when a large review has many viewed files.
    for offset = 1, #selected, 500 do
      local batch = vim.list_slice(selected, offset, math.min(offset + 499, #selected))
      local args = {
        "git",
        "--no-optional-locks",
        "--literal-pathspecs",
        "-C",
        state.git_root,
        "ls-files",
        "--stage",
        "-z",
        "--",
      }
      for _, file in ipairs(batch) do
        args[#args + 1] = file.path
      end
      vim.system(args, { text = false, timeout = 5000 }, function(result)
        vim.schedule(function()
          local entries = {}
          for _, record in
            ipairs(vim.split(result.stdout or "", "\0", { plain = true, trimempty = true }))
          do
            local mode, oid, stage, path = record:match("^(%d+) (%x+) (%d)\t(.*)$")
            if path then
              entries[path] = stage == "0" and (mode == "100644" or mode == "100755") and oid
                or "unavailable"
            end
          end
          for _, file in ipairs(batch) do
            local valid, identity = pcall(vim.json.decode, state.reviewed[file.path] or "")
            local expected = file.index_oid
            if expected == nil and valid then
              expected = identity.new and identity.new.blob_oid or false
            end
            invalidate(file, result.code ~= 0 or (entries[file.path] or false) ~= expected)
          end
        end)
      end)
    end
  end
end

return M
