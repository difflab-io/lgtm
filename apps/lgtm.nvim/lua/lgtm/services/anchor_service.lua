-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local rpc = require("lgtm.services.client")
local anchors = require("lgtm.models.source")

-- Methods ---------------------------------------------------------------------

-- Only the editor can establish that its displayed buffer still matches saved bytes.
function M.check_buffer(view, buf)
  if not view or not view.source or view.content == nil then
    return { message = "Selected source is unavailable" }
  end
  if vim.bo[buf].buftype == "" and vim.bo[buf].modified then
    return { message = "Unsaved buffer. Save to restore comment locations and add comments." }
  end
  local lines = anchors.lines(view.content)
  local displayed = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  for index, line in ipairs(displayed) do
    if line:sub(-1) == "\r" and lines[index] == line:sub(1, -2) then
      displayed[index] = line:sub(1, -2)
    end
  end
  if not vim.deep_equal(#lines == 0 and { "" } or lines, displayed) then
    return {
      message = "Displayed text differs from the saved source. Save and refresh before commenting.",
    }
  end
end

function M.capture(view, buf, kind, first, last, callback)
  if kind == "general" then
    callback(nil, { kind = "general" })
    return
  end
  local mismatch = M.check_buffer(view, buf)
  if mismatch then
    callback(mismatch)
    return
  end
  local tick = vim.api.nvim_buf_get_changedtick(buf)
  local input = vim.tbl_extend("force", view.selection, {
    location = {
      kind = kind,
      path = view.path,
      side = kind == "line" and view.side or nil,
      start = kind == "line" and first or nil,
      ["end"] = kind == "line" and last or nil,
    },
    side = view.side,
  })
  rpc.call("CaptureAnchor", input, function(err, result)
    if not vim.api.nvim_buf_is_valid(buf) or vim.api.nvim_buf_get_changedtick(buf) ~= tick then
      callback({ message = "Buffer changed while capturing the comment location. Try again." })
      return
    end
    if
      not err and not vim.deep_equal(anchors.source_from_wire(result.anchor.source), view.source)
    then
      callback({ message = "Saved source changed. Refresh before commenting." })
      return
    end
    callback(err, result and result.anchor)
  end)
end

---@param state lgtm.ReviewState
function M.get_placements(state, callback, conversation)
  local reader = state.placement_read
  if reader then
    reader.callbacks[#reader.callbacks + 1] = callback
    reader.conversation = conversation or reader.conversation
    reader.again = true
    return
  end
  reader = { callbacks = { callback }, conversation = conversation or state.conversation }
  state.placement_read = reader
  local refresh
  refresh = function()
    reader.again = false
    local views, candidate = state.views, reader.conversation
    local input = { views = {}, notes = state.note == true }
    for _, side in ipairs({ "old", "new" }) do
      local view = views and views[side]
      if view then
        input.views[#input.views + 1] = {
          path = view.path,
          logicalPath = state.kind == "directory"
              and vim.fs.joinpath(state.root, view.logical_path)
            or view.logical_path,
          previousPath = view.previous_path,
          side = side,
          confirmedRename = view.confirmed_rename,
          source = view.source and anchors.source_to_wire(view.source),
          content = view.content,
          comparison = view.comparison,
          historical = view.historical,
          unavailable = view.unavailable,
        }
      end
    end
    local placements, offset = {}, 1
    local threads = candidate and candidate.threads or {}
    local function finish(err)
      if reader.again or state.views ~= views then
        refresh()
        return
      end
      if not err then
        require("lgtm.services.threads_service").publish(state, candidate, placements, views)
      end
      state.placement_read = nil
      for _, done in ipairs(reader.callbacks) do
        done(err)
      end
    end
    local function page()
      local batch = vim.list_slice(threads, offset, offset + 499)
      input.anchors = vim.tbl_map(function(thread)
        return {
          original = thread.anchor,
          currentPath = thread.currentPath,
          forgeObservation = thread.forgeObservation,
        }
      end, batch)
      rpc.call("GetAnchorPlacements", input, function(err, response)
        if err or reader.again or state.views ~= views then
          finish(err)
          return
        end
        for _, value in ipairs(response.placements or {}) do
          local thread = batch[tonumber(value.index or 0) + 1]
          placements[thread.id] = placements[thread.id] or {}
          placements[thread.id][value.side] = Private.from_wire(value)
        end
        offset = offset + #batch
        if offset <= #threads then
          page()
        else
          finish()
        end
      end)
    end
    if #threads == 0 or #input.views == 0 then
      finish()
    else
      page()
    end
  end
  refresh()
end

-- Private ---------------------------------------------------------------------

function Private.from_wire(value)
  for _, status in ipairs({ "exact", "relocated", "outdated", "orphaned", "unverified", "stale" }) do
    if value[status] then
      local location = value[status].location
      if location then
        location.start, location["end"] = tonumber(location.start), tonumber(location["end"])
      end
      return { status = status, location = location, reason = value[status].reason }
    end
  end
end
return M
