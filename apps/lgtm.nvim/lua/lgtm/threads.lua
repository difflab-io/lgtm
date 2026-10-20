local M = {
  namespace = vim.api.nvim_create_namespace("lgtm.comments"),
  padding_namespace = vim.api.nvim_create_namespace("lgtm.comment-padding"),
  expanded = {},
  visible = true,
}
local api = vim.api
local function comments(state, thread)
  local result, deleted_root = {}, nil
  for _, c in ipairs(state.conversation.comments or {}) do
    if c.threadId == thread.id then
      if c.deleted and tonumber(c.position or 0) == 0 then
        deleted_root = c
      end
      if not c.deleted and (c.remotePresence ~= "missing" or c.visibility == "draft") then
        result[#result + 1] = c
      end
    end
  end
  if deleted_root and #result > 0 then
    local placeholder = vim.deepcopy(deleted_root)
    placeholder.body, placeholder.read_only = "[Top-level comment deleted]", true
    result[#result + 1] = placeholder
  end
  table.sort(result, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  return result
end

-- Resolve against the selected source every time; raw stored ranges are never cursor targets.
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
    -- The backend restricts a saved-file scope to that file, including legacy directory storage.
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
  return require("lgtm.anchors").resolve(thread, view)
end

function M.at(state, path, side, line)
  local result = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    if placement and #comments(state, thread) > 0 and placement.status ~= "orphaned" then
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

local function comment_status(comment)
  if comment.visibility ~= "draft" then
    return comment.visibility or ""
  end
  if comment.postingStatus == "uncertain" or comment.postingStatus == "sending" then
    return "Sync uncertain"
  end
  if comment.remotePresence == "missing" then
    return "Saved draft · removed on GitHub"
  end
  return comment.forgeCommentId and comment.forgeCommentId ~= "" and "GitHub draft" or "Local draft"
end

local function blocks(state, buf, path, side, width)
  local groups = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local placement = M.placement(state, thread, path, side)
    local cs = comments(state, thread)
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
          local next_status = comment_status(c)
          if
            not status
            or next_status == "Sync uncertain"
            or (status ~= "Sync uncertain" and next_status == "Local draft")
          then
            status = next_status
          end
        end
      end
      vim.list_extend(
        groups[line].rows,
        require("lgtm.cards").render(thread, cs, status, width, M.expanded[thread.id], placement)
      )
    end
  end
  local result = vim.tbl_values(groups)
  table.sort(result, function(a, b)
    return a.line < b.line
  end)
  return result
end

local function put(buf, line, rows, padding)
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

function M.render(state, buf, path, side)
  if not api.nvim_buf_is_valid(buf) then
    return
  end
  api.nvim_buf_clear_namespace(buf, M.namespace, 0, -1)
  api.nvim_buf_clear_namespace(buf, M.padding_namespace, 0, -1)
  if not M.visible then
    return
  end
  require("lgtm.cards").highlights()
  local windows = vim.fn.win_findbuf(buf)
  local width = windows[1] and api.nvim_win_get_width(windows[1]) or 80
  for _, block in ipairs(blocks(state, buf, path, side, width)) do
    put(buf, block.line, block.rows)
  end
end

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
  require("lgtm.cards").highlights()
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
    for _, block in ipairs(blocks(state, pane.buf, path, side, api.nvim_win_get_width(pane.win))) do
      local mapped = require("lgtm.cards").other_line(
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
      put(visible[side].buf, event[side .. "_line"], rows, not own)
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

function M.mutate(state, method, input, done)
  if state.initializing then
    local err = { message = "PR comments are still loading; wait for Review ready" }
    if done then
      done(err)
    else
      require("lgtm.notifications").error(err)
    end
    return
  end
  input.scope = state.scope
  local object
  for _, comment in ipairs((state.conversation or {}).comments or {}) do
    if comment.id == input.commentId then
      object = comment
    end
  end
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    if thread.id == input.threadId then
      object = thread
    end
  end
  input.expectedVersion = object and object.version or state.version
  state.pending_mutations = state.pending_mutations or {}
  local fingerprint = method
    .. ":"
    .. vim.json.encode({
      scope = input.scope,
      commentId = input.commentId,
      threadId = input.threadId,
      body = input.body,
      resolved = input.resolved,
    })
  local saved = state.pending_mutations[fingerprint]
  if saved then
    input = saved
  else
    input.requestKey = require("lgtm.context").request_key()
    state.pending_mutations[fingerprint] = input
  end
  require("lgtm.rpc").call(method, input, function(err, result)
    if err then
      if done then
        done(err)
      else
        require("lgtm.notifications").error(err)
      end
      return
    end
    state.pending_mutations[fingerprint] = nil
    state.refresh(function(refresh_err)
      if not refresh_err then
        if result and result.syncError and result.syncError ~= "" then
          require("lgtm.notifications").error(
            "Reply saved locally; GitHub sync failed: "
              .. result.syncError
              .. ". Use :Lgtm sync to recover."
          )
        else
          require("lgtm.notifications").show("Comment updated")
        end
      end
      if done then
        done(refresh_err)
      end
    end)
  end)
end

-- Hidden files are not orphans. Only confirmed source or provider evidence gives that status.
function M.description(state, thread)
  if thread.orphanReason == "file_deleted" then
    return "Orphaned", "File was deleted; the discussion is retained"
  end
  if thread.orphanReason == "working_source_changed" then
    return "Orphaned", "Unstaged source changed; original evidence is retained"
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

function M.open(state, threads, pane)
  threads = vim.tbl_filter(function(thread)
    return #comments(state, thread) > 0
  end, threads)
  local origin = api.nvim_get_current_win()
  local cursor = api.nvim_win_get_cursor(origin)
  local buf = api.nvim_create_buf(false, true)
  local win
  local rows, lines, entries, spans = {}, {}, {}, {}
  local width = math.max(20, math.min(pane and 42 or 86, vim.o.columns - 6))
  for index, t in ipairs(threads) do
    local cs = comments(state, t)
    local banner = (t.state == "resolved" and "RESOLVED" or "OPEN")
      .. " · "
      .. (#cs - 1)
      .. (#cs == 2 and " reply" or " replies")
    if #threads > 1 then
      banner = "THREAD " .. index .. " / " .. #threads .. " · " .. banner
    end
    if pane then
      local a = t.anchor or {}
      banner = banner .. " · " .. (a.path or "Review") .. (a.start and ":" .. a.start or "")
    end
    local location_status, location_reason = M.description(state, t)
    if location_status then
      banner = location_status:upper() .. " · " .. banner
    end
    lines[#lines + 1] = " " .. banner
    spans[#spans + 1] =
      { line = #lines, group = t.state == "resolved" and "LgtmSuccess" or "LgtmInfo" }
    rows[#lines] = { thread = t }
    lines[#lines + 1] = string.rep("─", width)
    spans[#spans + 1] = { line = #lines, group = "LgtmBorder" }
    rows[#lines] = { thread = t }
    if location_reason then
      lines[#lines + 1] = " " .. location_reason
      rows[#lines] = { thread = t }
      spans[#spans + 1] = { line = #lines, group = "LgtmWarning" }
      if t.anchor.start then
        lines[#lines + 1] = " Original lines " .. t.anchor.start .. "–" .. t.anchor["end"]
        rows[#lines] = { thread = t }
      end
      local excerpt = t.anchor.evidence and t.anchor.evidence.selectedText
      if excerpt and excerpt ~= "" then
        local source_lines = vim.split(excerpt, "\n", { plain = true })
        for index = 1, math.min(6, #source_lines) do
          lines[#lines + 1] = " > " .. source_lines[index]
          rows[#lines] = { thread = t }
        end
      end
      lines[#lines + 1] = ""
    end
    for i, c in ipairs(cs) do
      local role = tonumber(c.position or 0) == 0 and "top-level comment" or "reply " .. (i - 1)
      local entry =
        { thread = t, comment = c, role = role, line = #lines + 1, index = #entries + 1 }
      entries[#entries + 1] = entry
      local prefix = i == 1 and " " or "   └─ "
      lines[#lines + 1] = prefix
        .. role:upper()
        .. " · "
        .. (c.author and c.author.name or "Unknown")
        .. " · "
        .. comment_status(c)
      spans[#spans + 1] = { line = #lines, group = i == 1 and "LgtmTitle" or "LgtmMuted" }
      rows[#lines] = entry
      for _, text in ipairs(vim.split(c.body, "\n", { plain = true })) do
        lines[#lines + 1] = (i == 1 and "   " or "      ") .. text
        rows[#lines] = entry
      end
      entry.last_line = #lines
      lines[#lines + 1] = ""
      rows[#lines] = entry
    end
  end
  if #lines == 0 then
    lines = { "No comments" }
  end
  api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].filetype = "lgtm"
  vim.bo[buf].syntax = "markdown"
  vim.bo[buf].modifiable = false
  vim.bo[buf].bufhidden = "wipe"
  if pane then
    vim.cmd("botright 42vsplit")
    win = api.nvim_get_current_win()
    api.nvim_win_set_buf(win, buf)
  else
    local display_rows = 0
    for _, line in ipairs(lines) do
      display_rows = display_rows + math.max(1, math.ceil(vim.fn.strdisplaywidth(line) / width))
    end
    local height = math.max(3, math.min(display_rows, vim.o.lines - 7))
    win = require("lgtm.ui").open(buf, true, {
      relative = "editor",
      row = math.max(0, math.floor((vim.o.lines - height - 2) / 2)),
      col = math.max(0, math.floor((vim.o.columns - width - 2) / 2)),
      width = width,
      height = height,
      border = "rounded",
      title = #threads == 1 and " Review thread " or " Review threads ",
      footer = " c reply · e edit · dd delete · x resolve · q close ",
    })
  end
  vim.wo[win].number = false
  vim.wo[win].relativenumber = false
  vim.wo[win].signcolumn = "no"
  vim.wo[win].foldcolumn = "0"
  vim.wo[win].wrap = true
  vim.wo[win].linebreak = true
  vim.wo[win].cursorline = false
  vim.keymap.set("n", "j", "gj", { buffer = buf, silent = true })
  vim.keymap.set("n", "k", "gk", { buffer = buf, silent = true })
  local function selected()
    return rows[api.nvim_win_get_cursor(win)[1]]
  end
  local selection_namespace = api.nvim_create_namespace("lgtm.thread-selection")
  api.nvim_set_hl(0, "LgtmThreadSelection", { link = "LgtmSelection", default = true })
  for _, span in ipairs(spans) do
    api.nvim_buf_set_extmark(
      buf,
      selection_namespace,
      span.line - 1,
      0,
      { end_col = #lines[span.line], hl_group = span.group, priority = 120 }
    )
  end
  local active_mark
  local function selection()
    if not api.nvim_win_is_valid(win) then
      return
    end
    if active_mark then
      api.nvim_buf_del_extmark(buf, selection_namespace, active_mark)
    end
    local entry = selected()
    local label = "Select a comment with Tab"
    if entry and entry.comment then
      label = entry.role == "top-level comment" and "Top-level comment"
        or entry.role:gsub("^reply", "Reply")
      active_mark = api.nvim_buf_set_extmark(buf, selection_namespace, entry.line - 1, 0, {
        end_row = entry.last_line,
        hl_group = "LgtmThreadSelection",
        hl_eol = true,
        priority = 110,
      })
    end
    if not pane then
      api.nvim_win_set_config(win, {
        footer = " "
          .. label
          .. " · Tab next · c reply · e edit · dd delete · x resolve · q close ",
      })
    end
  end
  api.nvim_create_autocmd("CursorMoved", { buffer = buf, callback = selection })
  if entries[1] then
    api.nvim_win_set_cursor(win, { entries[1].line, 0 })
  end
  selection()
  local function close()
    if api.nvim_win_is_valid(win) then
      api.nvim_win_close(win, true)
    end
    if api.nvim_win_is_valid(origin) then
      api.nvim_set_current_win(origin)
      pcall(api.nvim_win_set_cursor, origin, cursor)
    end
  end
  local function map(key, callback)
    vim.keymap.set(
      "n",
      require("lgtm.config").options.mappings[key] or key,
      vim.schedule_wrap(callback),
      { buffer = buf, silent = true }
    )
  end
  local function owned(row)
    if not row or not row.comment then
      require("lgtm.notifications").show("Select a comment first")
      return false
    end
    if row.comment.read_only then
      require("lgtm.notifications").show("This comment was deleted; replies remain available")
      return false
    end
    local owner = row.comment.ownerId
    if not owner or owner == "" then
      owner = row.comment.author and row.comment.author.id
    end
    if owner ~= require("lgtm.rpc").identity then
      require("lgtm.notifications").error("Only your own comments can be changed")
      return false
    end
    return true
  end
  local function move_comment(direction)
    local entry = selected()
    local index = entry and entry.index or (direction == 1 and 0 or 1)
    if #entries > 0 then
      local next_entry = entries[(index - 1 + direction) % #entries + 1]
      api.nvim_win_set_cursor(win, { next_entry.line, 0 })
      selection()
    end
  end
  map("<Tab>", function()
    move_comment(1)
  end)
  map("<S-Tab>", function()
    move_comment(-1)
  end)
  map("q", close)
  map("<Esc>", close)
  map("c", function()
    local row = selected()
    if not row then
      return
    end
    require("lgtm.composer").open("Reply in thread", "", function(body, done)
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
    require("lgtm.composer").open("Edit " .. row.role, row.comment.body, function(body, done)
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
          M.mutate(state, "DeleteComment", { commentId = row.comment.id }, function(err)
            if err then
              require("lgtm.notifications").error(err)
            else
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
    M.mutate(
      state,
      "ResolveThread",
      { threadId = row.thread.id, resolved = row.thread.state ~= "resolved" },
      function(err)
        if err then
          require("lgtm.notifications").error(err)
        else
          close()
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
      close()
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
        end)
      end
    end)
  end
  M.last = { buf = buf, win = win, rows = rows, entries = entries }
  return M.last
end

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
return M
