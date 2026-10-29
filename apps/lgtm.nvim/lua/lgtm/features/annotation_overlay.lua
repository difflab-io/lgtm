-- State -----------------------------------------------------------------------

local M = {
  namespace = vim.api.nvim_create_namespace("lgtm.comments"),
  padding_namespace = vim.api.nvim_create_namespace("lgtm.comment-padding"),
  expanded = {},
  visible = true,
}
local Private = {}
local api = vim.api

-- Membership in a comparison is independent of whether a discussion is resolved or stale.

-- Resolve against the selected source every time; raw stored ranges are never cursor targets.

-- Render ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.render(state, buf, path, side)
  if not api.nvim_buf_is_valid(buf) then
    return
  end
  api.nvim_buf_clear_namespace(buf, M.namespace, 0, -1)
  api.nvim_buf_clear_namespace(buf, M.padding_namespace, 0, -1)
  if not M.visible then
    return
  end
  require("lgtm.ui.cards").highlights()
  local windows = vim.fn.win_findbuf(buf)
  local width = windows[1] and api.nvim_win_get_width(windows[1]) or 80
  for _, block in ipairs(Private.blocks(state, buf, path, side, width)) do
    Private.put(buf, block.line, block.rows)
  end
end

---@param state lgtm.ReviewState
function M.render_review(state, panes, path)
  local visible = {}
  for side, pane in pairs(panes) do
    if pane.buf and api.nvim_buf_is_valid(pane.buf) then
      api.nvim_buf_clear_namespace(pane.buf, M.namespace, 0, -1)
      api.nvim_buf_clear_namespace(pane.buf, M.padding_namespace, 0, -1)
      if pane.win and api.nvim_win_is_valid(pane.win) then
        visible[side] = pane
      end
    end
  end
  if not M.visible then
    return
  end
  require("lgtm.ui.cards").highlights()
  if not visible.old or not visible.new or visible.old.buf == visible.new.buf then
    for side, pane in pairs(visible) do
      M.render(state, pane.buf, path, side)
    end
    return
  end
  local session = require("codediff.ui.lifecycle").get_session(state.tab)
  local changes = session and session.stored_diff_result and session.stored_diff_result.changes
    or {}
  local events = {}
  for _, side in ipairs({ "old", "new" }) do
    local pane, other = visible[side], visible[side == "old" and "new" or "old"]
    for _, block in
      ipairs(Private.blocks(state, pane.buf, path, side, api.nvim_win_get_width(pane.win)))
    do
      local mapped = require("lgtm.features.diff_view").other_line(
        block.line,
        side,
        changes,
        api.nvim_buf_line_count(other.buf)
      )
      local left, right =
        side == "old" and block.line or mapped, side == "new" and block.line or mapped
      local key = left .. ":" .. right
      events[key] = events[key] or { old_line = left, new_line = right, old = {}, new = {} }
      vim.list_extend(events[key][side], block.rows)
    end
  end
  local ordered = vim.tbl_values(events)
  table.sort(ordered, function(a, b)
    return a.old_line == b.old_line and a.new_line < b.new_line or a.old_line < b.old_line
  end)
  for _, event in ipairs(ordered) do
    local height = math.max(#event.old, #event.new)
    for _, side in ipairs({ "old", "new" }) do
      local rows = event[side]
      local own = #rows > 0
      for _ = #rows + 1, height do
        rows[#rows + 1] = { { "", "Normal" } }
      end
      Private.put(visible[side].buf, event[side .. "_line"], rows, not own)
    end
  end
  for _, pane in pairs(visible) do
    api.nvim_win_call(pane.win, function()
      local view = vim.fn.winsaveview()
      if view.topline == 1 then
        local topfill = 0
        for _, mark in
          ipairs(api.nvim_buf_get_extmarks(pane.buf, -1, { 0, 0 }, { 0, -1 }, { details = true }))
        do
          if mark[4].virt_lines_above then
            topfill = topfill + #(mark[4].virt_lines or {})
          end
        end
        view.topfill = topfill
        vim.fn.winrestview(view)
      end
    end)
  end
end

-- Lifecycle -------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.attach(state, buf, path, side)
  local function at()
    return M.at(state, path, side, api.nvim_win_get_cursor(0)[1])
  end
  local function map(key, callback, opts)
    opts = vim.tbl_extend("force", { buffer = buf, silent = true }, opts or {})
    require("lgtm.bindings").set(
      state,
      "n",
      require("lgtm.config").options.mappings[key] or key,
      vim.schedule_wrap(callback),
      opts
    )
  end
  for _, key in ipairs({ "<CR>", "K" }) do
    local actual_key = require("lgtm.config").options.mappings[key] or key
    local binding = (state.bindings or {})[buf] and state.bindings[buf]["n:" .. actual_key]
    local existing = binding and binding.previous
      or api.nvim_buf_call(buf, function()
        return vim.fn.maparg(actual_key, "n", false, true)
      end)
    map(key, function()
      local ts = at()
      if #ts > 0 then
        require("lgtm.features.threads_pane").open_selected(state, ts)
        return
      end
      if existing.callback then
        existing.callback()
        return
      end
      vim.api.nvim_feedkeys(
        vim.api.nvim_replace_termcodes(
          existing.rhs or (key == "<CR>" and "<CR>" or "K"),
          true,
          false,
          true
        ),
        "n",
        false
      )
    end)
  end
  map("dd", function()
    local ts = at()
    if #ts > 0 then
      require("lgtm.features.threads_pane").open_selected(state, ts)
    elseif vim.bo[buf].modifiable then
      vim.cmd("normal! dd")
    else
      require("lgtm.ui.notifications").show("No comment at this line")
    end
  end)
  map("x", function()
    local ts = at()
    if #ts == 1 then
      require("lgtm.features.threads_pane").resolve(state, ts[1])
    elseif #ts > 1 then
      require("lgtm.features.threads_pane").open_selected(state, ts)
    elseif vim.bo[buf].modifiable then
      vim.cmd("normal! x")
    else
      require("lgtm.ui.notifications").show("No comment at this line")
    end
  end)
  map("za", function()
    local ts = at()
    if #ts == 0 then
      vim.cmd("normal! za")
      return
    end
    for _, t in ipairs(ts) do
      M.expanded[t.id] = not M.expanded[t.id]
    end
    if state.render then
      state.render()
    else
      M.render(state, buf, path, side)
    end
  end)
  map("zA", function()
    for _, t in ipairs((state.conversation or {}).threads or {}) do
      if t.anchor.path == path then
        M.expanded[t.id] = not M.expanded[t.id]
      end
    end
    if state.render then
      state.render()
    else
      M.render(state, buf, path, side)
    end
  end)
  map("gH", function()
    M.visible = not M.visible
    if state.render then
      state.render()
    end
  end)
  for key, direction in pairs({ ["]c"] = 1, ["[c"] = -1, ["]n"] = 1, ["[n"] = -1 }) do
    map(key, function()
      local line, candidates = api.nvim_win_get_cursor(0)[1], {}
      for _, t in ipairs((state.conversation or {}).threads or {}) do
        local placement = M.placement(state, t, path, side)
        if placement and placement.status ~= "orphaned" then
          candidates[#candidates + 1] = placement.location and placement.location.start or 1
        end
      end
      table.sort(candidates)
      if direction == -1 then
        candidates = vim.fn.reverse(candidates)
      end
      for _, target in ipairs(candidates) do
        if (target - line) * direction > 0 then
          pcall(api.nvim_win_set_cursor, 0, { target, 0 })
          return
        end
      end
    end)
  end
  M.render(state, buf, path, side)
end

-- Methods ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.placement(state, thread, path, side)
  local view = state.views and state.views[side]
  if not view or (not state.note and view.logical_path ~= path) then
    return nil
  end
  if state.placement_views ~= state.views or state.placement_conversation ~= state.conversation then
    return nil
  end
  return state.placements and state.placements[thread.id] and state.placements[thread.id][side]
end

---@param state lgtm.ReviewState
function M.at(state, path, side, line)
  local result = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    if
      placement
      and #require("lgtm.services.threads_service").get_comments(state, thread) > 0
      and placement.status ~= "orphaned"
    then
      local location = placement.location
      if
        (not location or location.kind == "file") and line == 1
        or location
          and location.kind == "line"
          and line >= location.start
          and line <= location["end"]
      then
        result[#result + 1] = thread
      end
    end
  end
  return result
end

---@param state lgtm.ReviewState
function M.description(state, thread)
  if thread.orphanReason == "file_deleted" then
    return "Orphaned", "File was deleted; the discussion is retained"
  end
  if M.is_outside_diff(state, thread) then
    return "Outside diff", "File is outside the current diff; the discussion is retained"
  end
  if thread.orphanReason == "working_source_changed" then
    return "Stale", "Unstaged source changed; original evidence is retained"
  end
  if thread.forgeObservation and thread.forgeObservation.outdated then
    return "Outdated", "Original PR selection is no longer current"
  end
  local path = thread.currentPath and thread.currentPath ~= "" and thread.currentPath
    or thread.anchor.path
  for _, side in ipairs({ "new", "old" }) do
    local placement = M.placement(state, thread, path, side)
    if placement and not placement.location then
      return placement.status:sub(1, 1):upper() .. placement.status:sub(2), placement.reason
    end
  end
end

---@param state lgtm.ReviewState
function M.is_outside_diff(state, thread)
  local anchor = thread.anchor or {}
  if state.note or not state.files or state.initializing or anchor.kind == "general" then
    return false
  end
  local path = thread.currentPath and thread.currentPath ~= "" and thread.currentPath or anchor.path
  if state.kind == "directory" and path and state.root then
    local prefix = state.root .. "/"
    if path:sub(1, #prefix) == prefix then
      path = path:sub(#prefix + 1)
    end
  end
  for _, file in ipairs(state.files) do
    if file.path == path or file.confirmedRename and file.previousPath == path then
      return false
    end
  end
  return path ~= nil and path ~= ""
end

-- Private ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function Private.blocks(state, buf, path, side, width)
  local groups = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    local cs = require("lgtm.services.threads_service").get_comments(state, thread)
    if placement and #cs > 0 and placement.status ~= "orphaned" then
      local line = placement.location
          and placement.location.kind == "line"
          and placement.location.start
        or 0
      if line > api.nvim_buf_line_count(buf) then
        error("Anchor resolver returned an out-of-range line")
      end
      groups[line] = groups[line] or { line = line, rows = {} }
      local status
      for _, c in ipairs(cs) do
        if c.visibility == "draft" then
          local next_status = require("lgtm.features.thread_comment").get_status(c)
          if
            not status
            or next_status == "Sync uncertain"
            or (status ~= "Sync uncertain" and next_status == "Local draft")
          then
            status = next_status
          end
        end
      end
      if not status and state.kind == "pr" then
        status = "Published"
      end
      vim.list_extend(
        groups[line].rows,
        require("lgtm.ui.cards").render({
          resolved = thread.state == "resolved",
          file = thread.anchor.kind == "file",
          status = status,
          location_status = placement and not placement.location and placement.status or nil,
          comments = vim.tbl_map(function(comment)
            return {
              body = comment.body,
              author = comment.author and comment.author.name or "Unknown",
              is_reply = tonumber(comment.position or 0) > 0,
            }
          end, cs),
        }, width, M.expanded[thread.id])
      )
    end
  end
  local result = vim.tbl_values(groups)
  table.sort(result, function(a, b)
    return a.line < b.line
  end)
  return result
end

function Private.put(buf, line, rows, padding)
  if #rows == 0 then
    return
  end
  api.nvim_buf_set_extmark(
    buf,
    padding and M.padding_namespace or M.namespace,
    math.max(0, line - 1),
    0,
    {
      virt_lines = rows,
      virt_lines_above = line == 0,
      virt_lines_leftcol = true,
      priority = 1100,
    }
  )
end
return M
