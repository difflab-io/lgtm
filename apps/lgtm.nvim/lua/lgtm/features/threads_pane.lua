local CommentEditor = require("lgtm.features.comment_editor")
local Notifications = require("lgtm.ui.notifications")
local NuiSplit = require("nui.split")
local ThreadDetailView = require("lgtm.ui.thread_detail_view")
local ThreadSummaryView = require("lgtm.ui.thread_summary_view")

local ThreadsPane = {}
local UI = {}
local Private = {}

-- State -----------------------------------------------------------------------

local ns_id = vim.api.nvim_create_namespace("lgtm_threads_pane")
local active_pane = nil

-- Render ----------------------------------------------------------------------

function UI.render(pane)
  if not pane or not pane.split or not pane.split.bufnr then
    return
  end
  local bufnr = pane.split.bufnr
  if not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end

  local snapshot = pane.service.state:get_value()
  local active_view = snapshot.active_view or "summary"
  local lines = {}
  pane.line_to_thread = {}
  pane.line_to_comment = {}

  if active_view == "detail" then
    local thread = pane.service:get_thread(snapshot.selected_thread_id) or snapshot.threads[1]
    lines, pane.line_to_comment = ThreadDetailView.build_lines(thread)
    vim.bo[bufnr].filetype = "lgtm-thread-detail"
  else
    lines, pane.line_to_thread = ThreadSummaryView.build_lines({
      mode = pane.mode,
      threads = snapshot.threads or {},
      selected_thread_id = snapshot.selected_thread_id,
    })
    vim.bo[bufnr].filetype = "lgtm-threads-summary"
  end

  Private.write_lines(bufnr, lines)
end

-- UI --------------------------------------------------------------------------

function UI.open_reply_editor(pane, thread)
  if not thread then
    return
  end
  CommentEditor.open({
    action = "reply",
    mode = pane.mode,
    thread = thread,
    on_submit = function(body)
      local updated = pane.service:reply_to_thread(thread.id, body)
      Notifications.reply_added(updated)
      UI.render(pane)
    end,
  })
end

function UI.open_edit_editor(pane, thread, comment)
  if not thread or not comment then
    return
  end
  CommentEditor.open({
    action = "edit",
    mode = pane.mode,
    thread = thread,
    comment = comment,
    initial_body = comment.body,
    on_submit = function(body)
      local updated = pane.service:edit_comment(thread.id, comment.id, body)
      Notifications.comment_updated(updated)
      UI.render(pane)
    end,
  })
end

-- Lifecycle -------------------------------------------------------------------

---Open or focus the right-hand `threads-pane` split for the given mode and service.
---@param opts { mode: "local"|"pr", service: table, view?: "summary"|"detail", thread_id?: string, enter?: boolean, on_jump?: fun(thread: LgtmThread) }
---@return table
function ThreadsPane.open(opts)
  assert(type(opts) == "table" and opts.service, "ThreadsPane.open requires a service")
  if opts.thread_id then
    if opts.view == "detail" then
      opts.service:open_thread_detail(opts.thread_id)
    else
      opts.service:select_thread(opts.thread_id)
    end
  elseif opts.view == "summary" then
    opts.service:close_thread_detail()
  end

  if
    active_pane
    and active_pane.split
    and active_pane.split.winid
    and vim.api.nvim_win_is_valid(active_pane.split.winid)
  then
    if active_pane.service ~= opts.service or active_pane.mode ~= opts.mode then
      Private.resubscribe(active_pane, opts.mode, opts.service)
    end
    active_pane.on_jump = opts.on_jump or active_pane.on_jump
    UI.render(active_pane)
    if opts.enter ~= false then
      vim.api.nvim_set_current_win(active_pane.split.winid)
    end
    return active_pane
  end

  local origin_win = vim.api.nvim_get_current_win()
  local split = NuiSplit({
    relative = "editor",
    position = "right",
    size = 58,
    enter = opts.enter ~= false,
    buf_options = {
      buftype = "nofile",
      bufhidden = "wipe",
      swapfile = false,
      modifiable = false,
    },
    win_options = {
      cursorline = true,
      number = false,
      relativenumber = false,
      signcolumn = "no",
      wrap = true,
    },
  })

  local pane = {
    mode = opts.mode or "local",
    service = opts.service,
    origin_win = origin_win,
    split = split,
    subscriptions = {},
    line_to_thread = {},
    line_to_comment = {},
    on_jump = opts.on_jump,
  }

  split:mount()
  active_pane = pane
  Private.attach_mappings(pane)
  Private.subscribe_signals(pane)

  split:on("BufWipeout", function()
    Private.unsubscribe_all(pane)
    if active_pane == pane then
      active_pane = nil
    end
  end)

  UI.render(pane)
  return pane
end

---Toggle the `threads-pane` split open or closed.
---@param opts { mode: "local"|"pr", service: table, on_jump?: fun(thread: LgtmThread) }
---@return table|nil
function ThreadsPane.toggle(opts)
  if ThreadsPane.is_open() then
    ThreadsPane.close()
    return nil
  end
  return ThreadsPane.open(opts)
end

---Close the active `threads-pane` split and unsubscribe from service signals.
function ThreadsPane.close()
  if not active_pane then
    return
  end
  local pane = active_pane
  active_pane = nil
  Private.unsubscribe_all(pane)
  if pane.split then
    pcall(function()
      pane.split:unmount()
    end)
  end
end

-- Methods ---------------------------------------------------------------------

---Return true when the `threads-pane` split window is currently valid and open.
---@return boolean
function ThreadsPane.is_open()
  return active_pane ~= nil
    and active_pane.split ~= nil
    and active_pane.split.winid ~= nil
    and vim.api.nvim_win_is_valid(active_pane.split.winid)
end

---Return the active `threads-pane` state table, or nil if closed.
---@return table|nil
function ThreadsPane.get_active()
  if ThreadsPane.is_open() then
    return active_pane
  end
  return nil
end

---Return the thread under the cursor in the pane (or the currently selected thread).
---@return LgtmThread|nil
function ThreadsPane.get_cursor_thread()
  if not ThreadsPane.is_open() then
    return nil
  end
  return Private.resolve_cursor_thread(active_pane)
end

---Return the comment under the cursor in `thread-detail-view`, or the last comment of the thread.
---@return LgtmComment|nil
function ThreadsPane.get_cursor_comment()
  if not ThreadsPane.is_open() then
    return nil
  end
  return Private.resolve_cursor_comment(active_pane)
end

-- Private ---------------------------------------------------------------------

function Private.subscribe_signals(pane)
  local redraw = vim.schedule_wrap(function()
    if active_pane == pane then
      UI.render(pane)
    end
  end)
  table.insert(pane.subscriptions, pane.service.state.threads:observe(redraw))
  table.insert(pane.subscriptions, pane.service.state.selected_thread_id:observe(redraw))
  table.insert(pane.subscriptions, pane.service.state.active_view:observe(redraw))
end

function Private.resubscribe(pane, mode, service)
  Private.unsubscribe_all(pane)
  pane.mode = mode
  pane.service = service
  Private.subscribe_signals(pane)
end

function Private.unsubscribe_all(pane)
  for _, sub in ipairs(pane.subscriptions or {}) do
    if sub and sub.unsubscribe then
      sub:unsubscribe()
    end
  end
  pane.subscriptions = {}
end

function Private.resolve_cursor_thread(pane)
  local snapshot = pane.service.state:get_value()
  if snapshot.active_view == "detail" then
    return pane.service:get_thread(snapshot.selected_thread_id) or snapshot.threads[1]
  end
  if pane.split.winid and vim.api.nvim_win_is_valid(pane.split.winid) then
    local row = vim.api.nvim_win_get_cursor(pane.split.winid)[1]
    if pane.line_to_thread[row] then
      return pane.line_to_thread[row]
    end
  end
  return pane.service:get_thread(snapshot.selected_thread_id) or snapshot.threads[1]
end

function Private.resolve_cursor_comment(pane)
  local thread = Private.resolve_cursor_thread(pane)
  if not thread or not thread.comments then
    return nil
  end
  if pane.split.winid and vim.api.nvim_win_is_valid(pane.split.winid) then
    local row = vim.api.nvim_win_get_cursor(pane.split.winid)[1]
    if pane.line_to_comment[row] then
      return pane.line_to_comment[row]
    end
  end
  return thread.comments[#thread.comments]
end

function Private.attach_mappings(pane)
  local bufnr = pane.split.bufnr
  local function map(lhs, rhs, desc)
    vim.keymap.set("n", lhs, rhs, {
      buffer = bufnr,
      nowait = true,
      silent = true,
      desc = desc,
    })
  end

  map("<CR>", function()
    local thread = Private.resolve_cursor_thread(pane)
    if not thread then
      return
    end
    pane.service:open_thread_detail(thread.id)
    Notifications.thread_opened(thread)
    UI.render(pane)
  end, "LGTM: Enter thread detail view")

  map("<BS>", function()
    pane.service:close_thread_detail()
    UI.render(pane)
  end, "LGTM: Return to threads summary view")

  map("h", function()
    if pane.service.state:get_value().active_view == "detail" then
      pane.service:close_thread_detail()
      UI.render(pane)
    end
  end, "LGTM: Return to threads summary view")

  map("za", function()
    local thread = Private.resolve_cursor_thread(pane)
    if thread then
      pane.service:toggle_collapsed(thread.id)
      UI.render(pane)
    end
  end, "LGTM: Toggle thread collapse")

  map("zA", function()
    local thread = Private.resolve_cursor_thread(pane)
    local target = thread and not thread.collapsed or true
    pane.service:set_all_collapsed(target)
    UI.render(pane)
  end, "LGTM: Toggle all threads collapse")

  map("x", function()
    local thread = Private.resolve_cursor_thread(pane)
    if thread then
      local updated = pane.service:toggle_resolved(thread.id)
      Notifications.thread_resolved(updated)
      UI.render(pane)
    end
  end, "LGTM: Toggle thread resolved")

  map("c", function()
    local thread = Private.resolve_cursor_thread(pane)
    UI.open_reply_editor(pane, thread)
  end, "LGTM: Reply to thread")

  map("r", function()
    local thread = Private.resolve_cursor_thread(pane)
    UI.open_reply_editor(pane, thread)
  end, "LGTM: Reply to thread")

  map("e", function()
    local thread = Private.resolve_cursor_thread(pane)
    local comment = Private.resolve_cursor_comment(pane)
    UI.open_edit_editor(pane, thread, comment)
  end, "LGTM: Edit comment")

  map("dd", function()
    local thread = Private.resolve_cursor_thread(pane)
    local comment = Private.resolve_cursor_comment(pane)
    if thread and comment then
      pane.service:delete_comment(thread.id, comment.id)
      Notifications.comment_deleted(thread)
      UI.render(pane)
    end
  end, "LGTM: Delete comment")

  map("gd", function()
    local thread = Private.resolve_cursor_thread(pane)
    if thread and pane.on_jump then
      pane.on_jump(thread)
    end
  end, "LGTM: Jump to thread file and line")

  map("q", function()
    ThreadsPane.close()
  end, "LGTM: Close threads pane")
end

function Private.write_lines(bufnr, lines)
  vim.bo[bufnr].modifiable = true
  vim.api.nvim_buf_clear_namespace(bufnr, ns_id, 0, -1)
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, {})
  for index, line in ipairs(lines) do
    line:render(bufnr, ns_id, index)
  end
  vim.bo[bufnr].modifiable = false
end

return ThreadsPane
