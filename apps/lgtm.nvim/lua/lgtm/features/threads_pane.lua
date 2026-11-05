local CommentEditor = require("lgtm.features.comment_editor")
local Notifications = require("lgtm.ui.notifications")
local Object = require("nui.object")
local Split = require("nui.split")
local ThreadDetailView = require("lgtm.ui.thread_detail_view")
local ThreadSummaryView = require("lgtm.ui.thread_summary_view")
local n = require("nui-components")

local ThreadsPane = Object("ThreadsPane")

local active_pane = nil

function ThreadsPane:init(opts)
  assert(type(opts) == "table" and opts.service, "ThreadsPane requires a service")
  self._private = {
    mode = opts.mode or "local",
    service = opts.service,
    enter = opts.enter ~= false,
    on_jump = opts.on_jump,
    origin_win = vim.api.nvim_get_current_win(),
    split = nil,
    renderer = nil,
    summary_view = nil,
    detail_view = nil,
    unmounting = false,
  }
end

function ThreadsPane:mount()
  local split = Split({
    relative = "editor",
    position = "right",
    size = 58,
    enter = false,
    buf_options = {
      buftype = "nofile",
      bufhidden = "wipe",
      swapfile = false,
      modifiable = false,
    },
    win_options = {
      number = false,
      relativenumber = false,
      signcolumn = "no",
    },
  })
  split:mount()

  local renderer = n.create_renderer({
    relative = {
      type = "win",
      winid = split.winid,
    },
    position = {
      row = 0,
      col = 0,
    },
    width = vim.api.nvim_win_get_width(split.winid),
    height = vim.api.nvim_win_get_height(split.winid),
    keymap = {
      close = "q",
    },
    on_unmount = function()
      self:_handle_unmount()
    end,
  })

  split:on("WinResized", function()
    if split.winid and vim.api.nvim_win_is_valid(split.winid) then
      renderer:set_size({
        width = vim.api.nvim_win_get_width(split.winid),
        height = vim.api.nvim_win_get_height(split.winid),
      })
    end
  end)

  split:on("BufWipeout", function()
    self:unmount()
  end)

  self._private.split = split
  self._private.renderer = renderer
  self.split = split
  self.renderer = renderer

  renderer:render(function()
    return self:render_body()
  end)

  return self
end

function ThreadsPane:render_body()
  local state = self._private.service.state

  self._private.summary_view = ThreadSummaryView({
    id = "lgtm_thread_summary_view",
    flex = 1,
    autofocus = self._private.enter,
    hidden = state.active_view:dup():map(function(view)
      return view == "detail"
    end),
    mode = self._private.mode,
    threads = state.threads:dup(),
    selected_thread_id = state.selected_thread_id:dup(),
    on_open_detail = function(thread)
      self:open_detail(thread)
    end,
    on_toggle_collapse = function(thread)
      self._private.service:toggle_collapsed(thread.id)
    end,
    on_toggle_all_collapse = function(collapsed)
      self._private.service:set_all_collapsed(collapsed)
    end,
    on_toggle_resolved = function(thread)
      self:toggle_resolved(thread)
    end,
    on_reply = function(thread)
      self:open_reply_editor(thread)
    end,
    on_edit = function(thread, comment)
      self:open_edit_editor(thread, comment)
    end,
    on_delete = function(thread, comment)
      self:delete_comment(thread, comment)
    end,
    on_jump = function(thread)
      self:jump_to_thread(thread)
    end,
    on_close = function()
      self:unmount()
    end,
  })

  self._private.detail_view = ThreadDetailView({
    id = "lgtm_thread_detail_view",
    flex = 1,
    autofocus = self._private.enter,
    hidden = state.active_view:dup():map(function(view)
      return view ~= "detail"
    end),
    threads = state.threads:dup(),
    selected_thread_id = state.selected_thread_id:dup(),
    on_back = function()
      self._private.service:close_thread_detail()
    end,
    on_reply = function(thread)
      self:open_reply_editor(thread)
    end,
    on_edit = function(thread, comment)
      self:open_edit_editor(thread, comment)
    end,
    on_delete = function(thread, comment)
      self:delete_comment(thread, comment)
    end,
    on_toggle_resolved = function(thread)
      self:toggle_resolved(thread)
    end,
    on_jump = function(thread)
      self:jump_to_thread(thread)
    end,
    on_close = function()
      self:unmount()
    end,
  })

  return n.rows(self._private.summary_view, self._private.detail_view)
end

function ThreadsPane:unmount()
  if self._private.unmounting then
    return
  end
  self._private.unmounting = true
  if active_pane == self then
    active_pane = nil
  end
  if self._private.renderer then
    pcall(function()
      self._private.renderer:close()
    end)
  end
  if self._private.split then
    pcall(function()
      self._private.split:unmount()
    end)
  end
end

function ThreadsPane:_handle_unmount()
  if active_pane == self then
    active_pane = nil
  end
  if not self._private.unmounting and self._private.split then
    self._private.unmounting = true
    pcall(function()
      self._private.split:unmount()
    end)
  end
end

function ThreadsPane:open_detail(thread)
  if not thread then
    return
  end
  self._private.service:open_thread_detail(thread.id)
  Notifications.thread_opened(thread)
end

function ThreadsPane:toggle_resolved(thread)
  if not thread then
    return
  end
  local updated = self._private.service:toggle_resolved(thread.id)
  Notifications.thread_resolved(updated)
end

function ThreadsPane:delete_comment(thread, comment)
  if not thread or not comment then
    return
  end
  self._private.service:delete_comment(thread.id, comment.id)
  Notifications.comment_deleted(thread)
end

function ThreadsPane:jump_to_thread(thread)
  if thread and self._private.on_jump then
    self._private.on_jump(thread)
  end
end

function ThreadsPane:open_reply_editor(thread)
  if not thread then
    return
  end
  CommentEditor.open({
    action = "reply",
    mode = self._private.mode,
    thread = thread,
    on_submit = function(body)
      local updated = self._private.service:reply_to_thread(thread.id, body)
      Notifications.reply_added(updated)
    end,
  })
end

function ThreadsPane:open_edit_editor(thread, comment)
  if not thread or not comment then
    return
  end
  CommentEditor.open({
    action = "edit",
    mode = self._private.mode,
    thread = thread,
    comment = comment,
    initial_body = comment.body,
    on_submit = function(body)
      local updated = self._private.service:edit_comment(thread.id, comment.id, body)
      Notifications.comment_updated(updated)
    end,
  })
end

function ThreadsPane:get_active_component()
  local view = self._private.service.state:get_value().active_view
  if view == "detail" then
    return self._private.detail_view
  end
  return self._private.summary_view
end

function ThreadsPane:get_bufnr()
  local cmp = self:get_active_component()
  if cmp and cmp.bufnr and vim.api.nvim_buf_is_valid(cmp.bufnr) then
    return cmp.bufnr
  end
  return self._private.split and self._private.split.bufnr or nil
end

function ThreadsPane:get_winid()
  local cmp = self:get_active_component()
  if cmp and cmp.winid and vim.api.nvim_win_is_valid(cmp.winid) then
    return cmp.winid
  end
  return self._private.split and self._private.split.winid or nil
end

function ThreadsPane:is_mounted()
  local split = self._private.split
  local cmp = self:get_active_component()
  return split ~= nil
    and split.winid ~= nil
    and vim.api.nvim_win_is_valid(split.winid)
    and cmp ~= nil
    and cmp.winid ~= nil
    and vim.api.nvim_win_is_valid(cmp.winid)
    and cmp.bufnr ~= nil
    and vim.api.nvim_buf_is_valid(cmp.bufnr)
end

function ThreadsPane:focus()
  local cmp = self:get_active_component()
  if cmp then
    cmp:focus()
  end
end

function ThreadsPane.open(opts)
  assert(type(opts) == "table" and opts.service, "ThreadsPane.open requires a service")
  if opts.thread_id then
    if opts.view == "detail" then
      opts.service:open_thread_detail(opts.thread_id)
    else
      opts.service:select_thread(opts.thread_id)
      opts.service:close_thread_detail()
    end
  else
    opts.service:close_thread_detail()
  end

  if active_pane and active_pane:is_mounted() then
    if
      active_pane._private.service == opts.service
      and active_pane._private.mode == (opts.mode or "local")
    then
      active_pane._private.on_jump = opts.on_jump or active_pane._private.on_jump
      if opts.enter ~= false then
        active_pane:focus()
      end
      return active_pane
    end
    active_pane:unmount()
  end

  local instance = ThreadsPane(opts)
  active_pane = instance
  instance:mount()
  return instance
end

function ThreadsPane.toggle(opts)
  if ThreadsPane.is_open() then
    ThreadsPane.close()
    return nil
  end
  return ThreadsPane.open(opts)
end

function ThreadsPane.close()
  if active_pane then
    active_pane:unmount()
  end
end

function ThreadsPane.is_open()
  return active_pane ~= nil and active_pane:is_mounted()
end

function ThreadsPane.get_active()
  if ThreadsPane.is_open() then
    return active_pane
  end
  return nil
end

function ThreadsPane.get_cursor_thread()
  if not ThreadsPane.is_open() then
    return nil
  end
  local cmp = active_pane:get_active_component()
  return cmp and cmp:get_cursor_thread() or nil
end

function ThreadsPane.get_cursor_comment()
  if not ThreadsPane.is_open() then
    return nil
  end
  local cmp = active_pane:get_active_component()
  return cmp and cmp:get_cursor_comment() or nil
end

return ThreadsPane
