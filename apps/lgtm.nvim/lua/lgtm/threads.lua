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
-- Public ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.placement(state, thread, path, side)
  local view = state.views and state.views[side]
  if not view or (not state.note and view.logical_path ~= path) then
    return nil
  end
  local anchor = thread.anchor or {}
  local observed = thread.forgeObservation
  local selected_side = observed
      and not observed.outdated
      and observed.current
      and observed.current.side
    or anchor.side
    or "new"
  if anchor.kind == "file" then
    selected_side = state.views.new and state.views.new.source and "new" or "old"
  end
  if side ~= selected_side or anchor.kind == "general" then
    return nil
  end
  if state.note then
    -- The backend restricts a saved-file scope to that file, including moved-file associations.
    view = vim.tbl_extend(
      "force",
      view,
      { logical_path = thread.currentPath ~= "" and thread.currentPath or anchor.path }
    )
  else
    if state.kind == "directory" and thread.currentPath and thread.currentPath:sub(1, 1) == "/" then
      view = vim.tbl_extend("force", view, { logical_path = vim.fs.joinpath(state.root, path) })
    end
    local original, evidence = require("lgtm.anchors").from_wire(anchor)
    if not require("lgtm.anchors").path_confirmed(original, evidence, thread.currentPath, view) then
      return nil
    end
  end
  local placement = require("lgtm.anchors").resolve(thread, view)
  local source = anchor.source and anchor.source.workingFile
  -- Retain changed working-source feedback without asserting that its old lines are current.
  if
    placement.status == "orphaned"
    and source
    and view.source
    and source.contentHash ~= view.source.content_hash
  then
    return { status = "stale", reason = "Unstaged source changed; original evidence is retained" }
  end
  return placement
end

---@param state lgtm.ReviewState
function M.at(state, path, side, line)
  local result = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    if placement and #Private.comments(state, thread) > 0 and placement.status ~= "orphaned" then
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
      local mapped = require("lgtm.codediff").other_line(
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

---@param state lgtm.ReviewState
function M.mutate(state, method, input, done)
  local notice = require("lgtm.notifications")
  local action = method == "DeleteComment" and "Deleting comment…"
    or method == "ResolveThread"
      and (input.resolved and "Resolving thread…" or "Reopening thread…")
  local pending = action and notice.show(action)
  require("lgtm.services.feedback").mutate(state, method, input, function(err, result)
    if err then
      if pending or not done then
        notice.error(err, pending)
      end
      if done then
        done(err, pending ~= nil)
      end
      return
    end
    local applied = method == "DeleteComment" or method == "ResolveThread"
    if applied then
      notice.show(
        method == "DeleteComment" and "Comment deleted"
          or input.resolved and "Thread resolved"
          or "Thread reopened",
        nil,
        pending
      )
    elseif result and result.syncError and result.syncError ~= "" then
      notice.error(
        "Reply saved locally; GitHub sync failed: "
          .. result.syncError
          .. ". Use :Lgtm sync to recover."
      )
    else
      notice.show("Comment updated")
    end
    if done then
      done()
    end
  end)
end

-- Hidden files are not orphans. Only confirmed source or provider evidence gives that status.
---@param state lgtm.ReviewState
function M.description(state, thread)
  if thread.orphanReason == "file_deleted" then
    return "Orphaned", "File was deleted; the discussion is retained"
  end
  if Private.outside_diff(state, thread) then
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
function M.open(state, threads, pane, on_ready)
  local ids = vim.tbl_map(function(thread)
    return thread.id
  end, threads)
  local view, requested
  local actions = {}
  local function find(thread_id, comment_id)
    for _, thread in ipairs((state.conversation or {}).threads or {}) do
      if thread.id == thread_id then
        local row = { thread = thread }
        for _, comment in ipairs(Private.comments(state, thread)) do
          if comment.id == comment_id then
            row.comment = comment
            local position = tonumber(comment.position or 0)
            row.role = position == 0 and "top-level comment" or "reply " .. position
          end
        end
        return row
      end
    end
  end
  local function selected()
    local id = requested or view.selected()
    return id and find(id.thread_id, id.comment_id)
  end
  local function present()
    local records = {}
    for _, id in ipairs(ids) do
      local row = find(id)
      local thread = row and row.thread
      local cs = thread and Private.comments(state, thread) or {}
      if thread and #cs > 0 then
        local location_status, reason = M.description(state, thread)
        local a = thread.anchor or {}
        local record = {
          id = id,
          status = thread.state == "resolved" and "RESOLVED" or "OPEN",
          resolved = thread.state == "resolved",
          replies = 0,
          location_status = location_status,
          location = (a.path or "Review") .. (a.start and ":" .. a.start or ""),
          details = {},
          comments = {},
          can_resolve = require("lgtm.services.feedback").can_resolve(state, thread),
        }
        if reason then
          record.details[#record.details + 1] = reason
          if a.start then
            record.details[#record.details + 1] = "Original lines " .. a.start .. "–" .. a["end"]
          end
          local excerpt = a.evidence and a.evidence.selectedText
          if excerpt and excerpt ~= "" then
            local lines = vim.split(excerpt, "\n", { plain = true })
            for index = 1, math.min(6, #lines) do
              record.details[#record.details + 1] = "> " .. lines[index]
            end
          end
        end
        for _, comment in ipairs(cs) do
          local position = tonumber(comment.position or 0)
          if position > 0 then
            record.replies = record.replies + 1
          end
          record.comments[#record.comments + 1] = {
            id = comment.id,
            root = position == 0,
            role = position == 0 and "top-level comment" or "reply " .. position,
            author = comment.author and comment.author.name or "Unknown",
            status = Private.comment_status(comment),
            body = comment.body,
          }
        end
        records[#records + 1] = record
      end
    end
    return records
  end
  local function snapshot(component)
    local rows, entries = {}, {}
    for line, id in pairs(component.rows) do
      local row = find(id.thread_id, id.comment_id)
      if row then
        row.line, row.last_line = id.line, id.last_line
        rows[line] = row
      end
    end
    for index, id in ipairs(component.entries) do
      local row = find(id.thread_id, id.comment_id)
      row.line, row.last_line, row.index = id.line, id.last_line, index
      entries[index] = row
    end
    M.last = {
      buf = component.buf,
      win = component.win,
      rows = rows,
      entries = entries,
      selected = component.selected(),
    }
  end
  local function close()
    view.close()
  end
  local function refresh()
    view.update(present())
  end
  local function map(key, callback)
    local names = { c = "reply", e = "edit", dd = "delete", x = "resolve", ["<CR>"] = "navigate" }
    actions[names[key]] = callback
  end
  local function owned(row)
    if not row or not row.comment then
      require("lgtm.notifications").show("Select a comment first")
      return false
    end
    if not require("lgtm.services.feedback").can_edit(row.comment) then
      require("lgtm.notifications").error("Only your own comments can be changed")
      return false
    end
    return true
  end
  map("c", function()
    local row = selected()
    if not row then
      return
    end
    require("lgtm.ui.composer").open("Reply in thread", "", function(body, done)
      M.mutate(state, "Reply", { threadId = row.thread.id, body = body }, function(err)
        if not err then
          close()
        end
        done(err)
      end)
    end)
  end)
  map("e", function()
    local row = selected()
    if not owned(row) then
      return
    end
    require("lgtm.ui.composer").open("Edit " .. row.role, row.comment.body, function(body, done)
      M.mutate(state, "EditComment", { commentId = row.comment.id, body = body }, function(err)
        if not err then
          close()
        end
        done(err)
      end)
    end)
  end)
  map("dd", function()
    local row = selected()
    if not owned(row) then
      return
    end
    vim.ui.select(
      { "Cancel", "Delete" },
      { prompt = "Delete " .. row.role .. "?" },
      function(choice)
        if choice == "Delete" then
          M.mutate(state, "DeleteComment", { commentId = row.comment.id }, function(err, reported)
            if err and not reported then
              require("lgtm.notifications").error(err)
            end
            if not err then
              close()
            end
          end)
        end
      end
    )
  end)
  map("x", function()
    local row = selected()
    if not row then
      return
    end
    if not require("lgtm.services.feedback").can_resolve(state, row.thread) then
      require("lgtm.notifications").show("Publish draft comments before resolving their thread")
      return
    end
    M.mutate(
      state,
      "ResolveThread",
      { threadId = row.thread.id, resolved = row.thread.state ~= "resolved" },
      function(err, reported)
        if err and not reported then
          require("lgtm.notifications").error(err)
        end
        if not err then
          if pane then
            refresh()
          else
            close()
          end
        end
      end
    )
  end)
  if pane then
    map("<CR>", function()
      local row = selected()
      if not row then
        return
      end
      if Private.outside_diff(state, row.thread) then
        require("lgtm.notifications").show(
          "File is outside the current diff; the discussion is retained"
        )
        return
      end
      if state.show_file and row.thread.anchor.path then
        local path = row.thread.currentPath ~= "" and row.thread.currentPath
          or row.thread.anchor.path
        -- Navigation selects a file; only a confirmed derived placement may choose a line.
        state.show_file(path, nil, function()
          for _, side in ipairs({ "new", "old" }) do
            local placement = M.placement(state, row.thread, path, side)
            if placement and placement.location and placement.location.kind == "line" then
              api.nvim_set_current_win(state.panes[side].win)
              api.nvim_win_set_cursor(0, { placement.location.start, 0 })
              return
            end
          end
          -- Stale and file-level feedback can select a file without asserting a line.
          for _, side in ipairs({ "new", "old" }) do
            local target = state.panes[side]
            local source_view = state.views and state.views[side]
            if
              target
              and target.win
              and api.nvim_win_is_valid(target.win)
              and source_view
              and source_view.source
            then
              api.nvim_set_current_win(target.win)
              api.nvim_win_set_cursor(target.win, { 1, 0 })
              local padding = 0
              for _, namespace in ipairs({ M.namespace, M.padding_namespace }) do
                for _, mark in
                  ipairs(
                    api.nvim_buf_get_extmarks(target.buf, namespace, 0, -1, { details = true })
                  )
                do
                  if mark[2] == 0 and mark[4].virt_lines_above then
                    padding = padding + #(mark[4].virt_lines or {})
                  end
                end
              end
              api.nvim_win_call(target.win, function()
                vim.fn.winrestview({ topline = 1, topfill = padding })
              end)
              return
            end
          end
        end)
      end
    end)
  end
  view = require("lgtm.ui.conversation").open({
    mode = pane and "split" or "modal",
    threads = present(),
    mappings = require("lgtm.config").options.mappings,
    on_action = function(action, thread_id, comment_id)
      requested = { thread_id = thread_id, comment_id = comment_id }
      if find(thread_id, comment_id) and actions[action] then
        actions[action]()
      end
      requested = nil
    end,
    on_navigate = function(thread_id)
      requested = { thread_id = thread_id }
      if actions.navigate then
        actions.navigate()
      end
      requested = nil
    end,
    on_change = snapshot,
    on_ready = on_ready,
  })
  return M.last
end

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
        M.open(state, ts)
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
      M.open(state, ts)
    elseif vim.bo[buf].modifiable then
      vim.cmd("normal! dd")
    else
      require("lgtm.notifications").show("No comment at this line")
    end
  end)
  map("x", function()
    local ts = at()
    if #ts == 1 then
      M.mutate(
        state,
        "ResolveThread",
        { threadId = ts[1].id, resolved = ts[1].state ~= "resolved" }
      )
    elseif #ts > 1 then
      M.open(state, ts)
    elseif vim.bo[buf].modifiable then
      vim.cmd("normal! x")
    else
      require("lgtm.notifications").show("No comment at this line")
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
-- Private ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function Private.comments(state, thread)
  local result = {}
  for _, c in ipairs(state.conversation.comments or {}) do
    if c.threadId == thread.id then
      result[#result + 1] = c
    end
  end
  table.sort(result, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  return result
end

---@param state lgtm.ReviewState
function Private.outside_diff(state, thread)
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

function Private.comment_status(comment)
  if comment.visibility ~= "draft" then
    return comment.visibility or ""
  end
  if comment.postingStatus == "uncertain" or comment.postingStatus == "sending" then
    return "Sync uncertain"
  end
  return comment.forgeCommentId and comment.forgeCommentId ~= "" and "GitHub draft" or "Local draft"
end

---@param state lgtm.ReviewState
function Private.blocks(state, buf, path, side, width)
  local groups = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    local cs = Private.comments(state, thread)
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
          local next_status = Private.comment_status(c)
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
