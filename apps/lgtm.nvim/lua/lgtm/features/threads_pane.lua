-- State -----------------------------------------------------------------------

local M = {}
local UI, Private = {}, {}
local api = vim.api
local threads = require("lgtm.services.threads_service")

-- Render ----------------------------------------------------------------------

function M.render(props)
  local nodes, visible = {}, {}
  for _, thread in ipairs((props.conversation or {}).threads or {}) do
    if not props.ids or vim.list_contains(props.ids, thread.id) then
      visible[#visible + 1] = thread
    end
  end
  for index, thread in ipairs(visible) do
    local comments = threads.get_comments(props.state, thread)
    if #comments > 0 then
      local status, reason =
        require("lgtm.features.annotation_overlay").description(props.state, thread)
      vim.list_extend(
        nodes,
        UI.thread({
          thread = thread,
          comments = comments,
          can_resolve = threads.can_resolve(props.state, thread),
          location_status = status,
          reason = reason,
          width = props.width,
          mode = props.mode,
          index = index,
          count = #visible,
        })
      )
    end
  end
  return nodes
end

-- UI --------------------------------------------------------------------------

UI.thread = require("lgtm.features.thread").render
UI.conversation = require("lgtm.ui.conversation").open
UI.composer = require("lgtm.ui.composer").open

-- Lifecycle -------------------------------------------------------------------

function M.open(state)
  state = state or require("lgtm.services.state").current
  if not state then
    require("lgtm.ui.notifications").error("Select a review first")
    return
  end
  return M.open_selected(state, nil, true)
end

function M.open_selected(state, selected, pane, on_ready)
  local mode = pane and "split" or "modal"
  local layout = require("nui-components.signal").create({
    width = math.max(1, math.min(pane and 42 or 86, vim.o.columns - 6)),
  })
  local context = {
    state = state,
    mode = mode,
  }
  local ids = not pane
      and selected
      and vim.tbl_map(function(thread)
        return thread.id
      end, selected)
    or nil
  local data = threads.get_data(state):combine_latest(layout.width, function(snapshot, width)
    return M.render({
      state = state,
      conversation = snapshot.conversation,
      ids = ids,
      width = width,
      mode = mode,
    })
  end)
  context.view = UI.conversation({
    mode = mode,
    title = selected and #selected == 1 and "Review thread" or "Review threads",
    data = data,
    mappings = require("lgtm.config").options.mappings,
    on_action = function(action, thread_id, comment_id)
      Private.action(context, action, thread_id, comment_id)
    end,
    on_navigate = function(thread_id)
      Private.action(context, "navigate", thread_id)
    end,
    on_change = function(component)
      Private.snapshot(context, component)
    end,
    on_resize = function(width)
      layout.width = width
    end,
    on_ready = on_ready,
  })
  return M.last
end

-- Methods ---------------------------------------------------------------------

function M.resolve(state, thread, done)
  local notice = require("lgtm.ui.notifications")
  local resolved = thread.state ~= "resolved"
  local pending = notice.show(resolved and "Resolving thread…" or "Reopening thread…")
  threads.resolve(state, thread.id, resolved, function(err)
    if err then
      notice.error(err, pending)
    else
      notice.show(resolved and "Thread resolved" or "Thread reopened", nil, pending)
    end
    if done then
      done(err)
    end
  end)
end

function M.navigate(state, thread)
  if require("lgtm.features.annotation_overlay").is_outside_diff(state, thread) then
    require("lgtm.ui.notifications").show(
      "File is outside the current diff; the discussion is retained"
    )
    return
  end
  if state.show_file and thread.anchor.path then
    local path = thread.currentPath ~= "" and thread.currentPath or thread.anchor.path
    -- Navigation selects a file; only a confirmed derived placement may choose a line.
    state.show_file(path, nil, function()
      for _, side in ipairs({ "new", "old" }) do
        local placement =
          require("lgtm.features.annotation_overlay").placement(state, thread, path, side)
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
          for _, namespace in ipairs({
            require("lgtm.features.annotation_overlay").namespace,
            require("lgtm.features.annotation_overlay").padding_namespace,
          }) do
            for _, mark in
              ipairs(api.nvim_buf_get_extmarks(target.buf, namespace, 0, -1, { details = true }))
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
end

-- Private ---------------------------------------------------------------------

function Private.find(context, thread_id, comment_id)
  for _, thread in ipairs((context.state.conversation or {}).threads or {}) do
    if thread.id == thread_id then
      local row = { thread = thread }
      for _, comment in ipairs(threads.get_comments(context.state, thread)) do
        if comment.id == comment_id then
          row.comment = comment
          row.role = tonumber(comment.position or 0) == 0 and "top-level comment"
            or "reply " .. comment.position
        end
      end
      return row
    end
  end
end

function Private.snapshot(context, component)
  local rows, entries = {}, {}
  for line, entry in pairs(component.rows) do
    local row = Private.find(context, entry.thread_id, entry.comment_id)
    if row then
      row.line, row.last_line = entry.line, entry.last_line
      rows[line] = row
    end
  end
  for index, entry in ipairs(component.entries) do
    local row = Private.find(context, entry.thread_id, entry.comment_id)
    if row then
      row.line, row.last_line, row.index = entry.line, entry.last_line, index
      entries[index] = row
    end
  end
  M.last = {
    buf = component.buf,
    win = component.win,
    rows = rows,
    entries = entries,
    selected = component.selected(),
  }
end

function Private.action(context, action, thread_id, comment_id)
  local row = Private.find(context, thread_id, comment_id)
  local handlers = {
    reply = Private.reply,
    edit = Private.edit,
    delete = Private.delete,
    resolve = Private.resolve,
    navigate = Private.navigate,
  }
  if row and handlers[action] then
    handlers[action](context, row)
  end
end

function Private.saved(context, done, err, result)
  local notice = require("lgtm.ui.notifications")
  if err then
    notice.error(err)
  elseif result and result.syncError and result.syncError ~= "" then
    notice.error(
      "Reply saved locally; GitHub sync failed: "
        .. result.syncError
        .. ". Use :Lgtm sync to recover."
    )
  else
    notice.show("Comment updated")
  end
  if not err then
    context.view.close()
  end
  done(err)
end

function Private.reply(context, row)
  UI.composer("Reply in thread", "", function(body, done)
    threads.reply(context.state, row.thread.id, body, function(err, result)
      Private.saved(context, done, err, result)
    end)
  end)
end

function Private.edit(context, row)
  if not row.comment then
    return
  end
  if not threads.can_edit(row.comment) then
    require("lgtm.ui.notifications").error("Only your own comments can be changed")
    return
  end
  UI.composer("Edit " .. row.role, row.comment.body, function(body, done)
    threads.edit(context.state, row.comment.id, body, function(err, result)
      Private.saved(context, done, err, result)
    end)
  end)
end

function Private.delete(context, row)
  if not row.comment then
    return
  end
  if not threads.can_edit(row.comment) then
    require("lgtm.ui.notifications").error("Only your own comments can be changed")
    return
  end
  vim.ui.select({ "Cancel", "Delete" }, { prompt = "Delete " .. row.role .. "?" }, function(choice)
    if choice ~= "Delete" then
      return
    end
    local notice = require("lgtm.ui.notifications")
    local pending = notice.show("Deleting comment…")
    threads.delete(context.state, row.comment.id, function(err)
      if err then
        notice.error(err, pending)
      else
        notice.show("Comment deleted", nil, pending)
        context.view.close()
      end
    end)
  end)
end

function Private.resolve(context, row)
  M.resolve(context.state, row.thread, function(err)
    if not err and context.mode == "modal" then
      context.view.close()
    end
  end)
end

function Private.navigate(context, row)
  if context.mode == "split" then
    M.navigate(context.state, row.thread)
  end
end
return M
