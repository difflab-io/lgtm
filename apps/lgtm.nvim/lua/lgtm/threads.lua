local M = {
  original_maps = {},
  namespace = vim.api.nvim_create_namespace("lgtm.comments"),
  expanded = {},
  visible = true,
}
local api = vim.api
local function comments(state, thread)
  local result = {}
  for _, c in ipairs(state.conversation.comments or {}) do
    if c.threadId == thread.id and c.remotePresence ~= "missing" then
      result[#result + 1] = c
    end
  end
  table.sort(result, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  return result
end

function M.at(state, path, side, line)
  local result = {}
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    local a = thread.anchor or {}
    if
      #comments(state, thread) > 0
      and a.path == path
      and (a.side or "new") == side
      and a.kind == "line"
      and line >= tonumber(a.start)
      and line <= tonumber(a["end"])
    then
      result[#result + 1] = thread
    end
  end
  return result
end

function M.render(state, buf, path, side)
  if not api.nvim_buf_is_valid(buf) then
    return
  end
  api.nvim_buf_clear_namespace(buf, M.namespace, 0, -1)
  if not M.visible then
    return
  end
  local groups = {}
  for _, t in ipairs((state.conversation or {}).threads or {}) do
    local a = t.anchor or {}
    if
      #comments(state, t) > 0
      and a.kind == "line"
      and a.path == path
      and (a.side or "new") == side
    then
      local line = math.min(tonumber(a.start), api.nvim_buf_line_count(buf))
      groups[line] = groups[line] or {}
      table.insert(groups[line], t)
    end
  end
  for line, threads in pairs(groups) do
    local count, resolved, draft, preview, expanded = 0, true, false, "", {}
    for _, t in ipairs(threads) do
      local cs = comments(state, t)
      count = count + #cs
      resolved = resolved and t.state == "resolved"
      if cs[1] then
        preview = cs[1].body:gsub("\n", " "):sub(1, 60)
      end
      for _, c in ipairs(cs) do
        draft = draft or c.visibility == "draft"
        if M.expanded[t.id] then
          for _, text in
            ipairs(
              vim.split(
                (c.author and c.author.name or "") .. ": " .. c.body,
                "\n",
                { plain = true }
              )
            )
          do
            expanded[#expanded + 1] = { { "  │ " .. text, "Comment" } }
          end
        end
      end
    end
    api.nvim_buf_set_extmark(buf, M.namespace, line - 1, 0, {
      virt_text = {
        {
          string.format(
            "  %s %s%d comments / %d threads · %s",
            resolved and "✓" or "●",
            draft and "draft " or "",
            count,
            #threads,
            preview
          ),
          "DiagnosticInfo",
        },
      },
      virt_lines = #expanded > 0 and expanded or nil,
      sign_text = resolved and "✓" or "●",
      sign_hl_group = "DiagnosticInfo",
    })
  end
end

function M.mutate(state, method, input, done)
  input.sessionId = state.session.id
  require("lgtm.rpc").call(method, input, function(err)
    if err then
      if done then
        done(err)
      else
        require("lgtm.notifications").error(err)
      end
      return
    end
    state.refresh(function(refresh_err)
      if not refresh_err then
        require("lgtm.notifications").show("Draft updated")
      end
      if done then
        done(refresh_err)
      end
    end)
  end)
end

function M.open(state, threads, pane)
  local origin = api.nvim_get_current_win()
  local cursor = api.nvim_win_get_cursor(origin)
  local buf = api.nvim_create_buf(false, true)
  local win
  local rows, lines = {}, {}
  for _, t in ipairs(threads) do
    local anchor = t.anchor or {}
    lines[#lines + 1] = string.format(
      "%s %s %s %s",
      t.state or "open",
      anchor.path or "General",
      anchor.side or "",
      anchor.start or ""
    )
    rows[#lines] = { thread = t }
    for _, c in ipairs(comments(state, t)) do
      lines[#lines + 1] = (c.author and c.author.name or "Unknown")
        .. " · "
        .. (c.visibility or "")
      rows[#lines] = { thread = t, comment = c }
      for _, text in ipairs(vim.split(c.body, "\n", { plain = true })) do
        lines[#lines + 1] = "  " .. text
        rows[#lines] = { thread = t, comment = c }
      end
      lines[#lines + 1] = ""
      rows[#lines] = { thread = t, comment = c }
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
    win = api.nvim_open_win(buf, true, {
      relative = "editor",
      row = 2,
      col = 3,
      width = math.max(20, math.min(86, vim.o.columns - 6)),
      height = math.max(3, math.min(#lines, vim.o.lines - 7)),
      border = "rounded",
      title = " LGTM threads ",
      footer = " c reply · e edit · dd delete · x resolve · q close ",
    })
  end
  local function selected()
    return rows[api.nvim_win_get_cursor(win)[1]]
  end
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
    if row.comment.author and row.comment.author.id ~= require("lgtm.rpc").identity then
      require("lgtm.notifications").error("Only your own comments can be changed")
      return false
    end
    return true
  end
  map("q", close)
  map("<Esc>", close)
  map("c", function()
    local row = selected()
    if not row then
      return
    end
    require("lgtm.composer").open("Reply", "", function(body, done)
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
    require("lgtm.composer").open("Edit comment", row.comment.body, function(body, done)
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
    vim.ui.select({ "Cancel", "Delete" }, { prompt = "Delete selected comment?" }, function(choice)
      if choice == "Delete" then
        M.mutate(state, "DeleteComment", { commentId = row.comment.id }, function(err)
          if err then
            require("lgtm.notifications").error(err)
          else
            close()
          end
        end)
      end
    end)
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
        state.show_file(row.thread.anchor.path, row.thread.anchor)
      end
    end)
  end
  M.last = { buf = buf, win = win, rows = rows }
  return M.last
end

function M.attach(state, buf, path, side)
  local function at()
    return M.at(state, path, side, api.nvim_win_get_cursor(0)[1])
  end
  local function map(key, callback, opts)
    opts = vim.tbl_extend("force", { buffer = buf, silent = true }, opts or {})
    vim.keymap.set(
      "n",
      require("lgtm.config").options.mappings[key] or key,
      vim.schedule_wrap(callback),
      opts
    )
  end
  for _, key in ipairs({ "<CR>", "K" }) do
    M.original_maps[buf] = M.original_maps[buf] or {}
    M.original_maps[buf][key] = M.original_maps[buf][key] or vim.fn.maparg(key, "n", false, true)
    local existing = M.original_maps[buf][key]
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
    else
      vim.cmd("normal! dd")
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
    else
      vim.cmd("normal! x")
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
    M.render(state, buf, path, side)
  end)
  map("zA", function()
    for _, t in ipairs((state.conversation or {}).threads or {}) do
      if t.anchor.path == path then
        M.expanded[t.id] = not M.expanded[t.id]
      end
    end
    M.render(state, buf, path, side)
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
        local a = t.anchor
        if a.kind == "line" and a.path == path and a.side == side then
          candidates[#candidates + 1] = tonumber(a.start)
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
