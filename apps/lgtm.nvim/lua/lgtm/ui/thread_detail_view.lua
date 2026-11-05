local AnchoredThreadSummary = require("lgtm.ui.anchored_thread_summary")
local Component = require("nui-components.component")
local Line = require("nui.line")
local Thread = require("lgtm.models.thread")
local fn = require("nui-components.utils.fn")

local ThreadDetailView = Component:extend("ThreadDetailView")

function ThreadDetailView:init(props, popup_options)
  props = fn.merge({
    flex = 1,
    threads = {},
    selected_thread_id = "",
    on_back = fn.ignore,
    on_reply = fn.ignore,
    on_edit = fn.ignore,
    on_delete = fn.ignore,
    on_toggle_resolved = fn.ignore,
    on_jump = fn.ignore,
    on_close = fn.ignore,
  }, props)

  popup_options = fn.deep_merge({
    buf_options = {
      filetype = "lgtm-thread-detail",
    },
    win_options = {
      cursorline = true,
      number = false,
      relativenumber = false,
      signcolumn = "no",
      wrap = true,
    },
  }, popup_options)

  ThreadDetailView.super.init(self, props, popup_options)

  self._private.namespace = vim.api.nvim_create_namespace("lgtm_detail_" .. self:get_id())
  self._private.line_to_comment = {}
end

function ThreadDetailView:prop_types()
  return {
    threads = "table",
    selected_thread_id = { "string", "nil" },
    on_back = "function",
    on_reply = "function",
    on_edit = "function",
    on_delete = "function",
    on_toggle_resolved = "function",
    on_jump = "function",
    on_close = "function",
  }
end

function ThreadDetailView:mappings()
  local props = self:get_props()

  return {
    {
      mode = { "n" },
      key = { "<BS>", "h" },
      handler = function()
        props.on_back(self)
      end,
    },
    {
      mode = { "n" },
      key = { "c", "r" },
      handler = function()
        local thread = self:get_cursor_thread()
        if thread then
          props.on_reply(thread, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "e",
      handler = function()
        local thread = self:get_cursor_thread()
        local comment = self:get_cursor_comment()
        if thread and comment then
          props.on_edit(thread, comment, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "dd",
      handler = function()
        local thread = self:get_cursor_thread()
        local comment = self:get_cursor_comment()
        if thread and comment then
          props.on_delete(thread, comment, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "x",
      handler = function()
        local thread = self:get_cursor_thread()
        if thread then
          props.on_toggle_resolved(thread, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "gd",
      handler = function()
        local thread = self:get_cursor_thread()
        if thread then
          props.on_jump(thread, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "q",
      handler = function()
        props.on_close(self)
      end,
    },
  }
end

function ThreadDetailView:on_mount()
  self:_render_buffer()
end

function ThreadDetailView:on_update()
  self:if_not_mounting_phase(function()
    self:modify_buffer_content(function()
      self:_render_buffer()
    end)
  end)
end

function ThreadDetailView:on_visibility_change()
  vim.schedule(function()
    local renderer = self:get_renderer()
    if not self:is_hidden() and renderer and renderer.layout and renderer.layout._.mounted then
      renderer:_determine_focusable_components()
      renderer.layout:update(renderer:get_layout_options(), renderer:_get_layout_box())
      self:focus()
      self:_render_buffer()
    end
  end)
end

function ThreadDetailView:get_cursor_thread()
  local props = self:get_props()
  local threads = props.threads or {}
  local selected_id = props.selected_thread_id
  for _, thread in ipairs(threads) do
    if thread.id == selected_id then
      return thread
    end
  end
  return threads[1]
end

function ThreadDetailView:get_cursor_comment()
  local thread = self:get_cursor_thread()
  if not thread or not thread.comments then
    return nil
  end
  if self.winid and vim.api.nvim_win_is_valid(self.winid) then
    local row = vim.api.nvim_win_get_cursor(self.winid)[1]
    local at_row = self._private.line_to_comment[row]
    if at_row then
      return at_row
    end
  end
  return thread.comments[#thread.comments]
end

function ThreadDetailView:get_lines()
  local thread = self:get_cursor_thread()
  return self:_build_detail_lines(thread)
end

function ThreadDetailView.build_lines(thread)
  return ThreadDetailView:_build_detail_lines(thread)
end

function ThreadDetailView:_render_buffer()
  if not self.bufnr or not vim.api.nvim_buf_is_valid(self.bufnr) then
    return
  end
  local lines, comment_map = self:get_lines()
  self._private.line_to_comment = comment_map
  vim.bo[self.bufnr].filetype = "lgtm-thread-detail"
  self:set_buffer_option("modifiable", true)
  vim.api.nvim_buf_clear_namespace(self.bufnr, self._private.namespace, 0, -1)
  vim.api.nvim_buf_set_lines(self.bufnr, 0, -1, false, {})
  for index, line in ipairs(lines) do
    line:render(self.bufnr, self._private.namespace, index)
  end
  self:set_buffer_option("modifiable", false)
end

function ThreadDetailView:_build_detail_lines(thread)
  local lines = {}
  local comment_map = {}
  if not thread then
    local empty = Line()
    empty:append(" No thread selected. Press <BS> to return to summary.", "Comment")
    table.insert(lines, empty)
    return lines, comment_map
  end

  for _, header_line in ipairs(self:_detail_header(thread)) do
    table.insert(lines, header_line)
  end

  for index, comment in ipairs(thread.comments or {}) do
    for _, comment_line in ipairs(self:_comment_block(comment, index)) do
      table.insert(lines, comment_line)
      comment_map[#lines] = comment
    end
  end

  return lines, comment_map
end

function ThreadDetailView:_detail_header(thread)
  AnchoredThreadSummary.ensure_highlights()
  local status = thread.resolved and "[RESOLVED]" or "[OPEN]"
  local status_hl = thread.resolved and "LgtmBadgeResolved" or "LgtmBadgeOpen"

  local title = Line()
  title:append(" ◀ Thread Detail · ", "Title")
  title:append(status, status_hl)
  title:append(" ", "Normal")
  if thread.outdated then
    title:append(" 󰦖 STALE ", "LgtmBadgeOutdated")
    title:append(" ", "Normal")
  end
  title:append(Thread.format_anchor(thread), "Directory")

  local hints = Line()
  hints:append(
    " [<BS>: summary · c/r: reply · e: edit · dd: delete · x: resolve · gd: jump]",
    "Comment"
  )

  local divider = Line()
  divider:append(" " .. string.rep("─", 56), "Comment")

  return { title, hints, divider }
end

function ThreadDetailView:_comment_block(comment, index)
  AnchoredThreadSummary.ensure_highlights()
  local header = Line()
  local label = index == 1 and "Root" or string.format("Reply #%d", index - 1)
  local state_label, state_hl = AnchoredThreadSummary.state_badge_spec(comment.state)
  header:append(string.format(" %s · @%s ", label, comment.author), "Title")
  header:append(state_label, state_hl)

  local rows = { header }
  local body = comment.body or ""
  local raw_lines = body ~= "" and vim.split(body, "\n", { plain = true }) or { "" }
  for _, text in ipairs(raw_lines) do
    local line = Line()
    line:append("   " .. text, "Normal")
    table.insert(rows, line)
  end
  local spacer = Line()
  spacer:append(" " .. string.rep("┄", 40), "Comment")
  table.insert(rows, spacer)
  return rows
end

return ThreadDetailView
