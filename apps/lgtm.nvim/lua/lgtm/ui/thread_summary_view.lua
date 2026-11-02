local Component = require("nui-components.component")
local Line = require("nui.line")
local Thread = require("lgtm.models.thread")
local fn = require("nui-components.utils.fn")

local ThreadSummaryView = Component:extend("ThreadSummaryView")

function ThreadSummaryView:init(props, popup_options)
  props = fn.merge({
    flex = 1,
    mode = "local",
    threads = {},
    selected_thread_id = "",
    on_open_detail = fn.ignore,
    on_toggle_collapse = fn.ignore,
    on_toggle_all_collapse = fn.ignore,
    on_toggle_resolved = fn.ignore,
    on_reply = fn.ignore,
    on_edit = fn.ignore,
    on_delete = fn.ignore,
    on_jump = fn.ignore,
    on_close = fn.ignore,
  }, props)

  popup_options = fn.deep_merge({
    buf_options = {
      filetype = "lgtm-threads-summary",
    },
    win_options = {
      cursorline = true,
      number = false,
      relativenumber = false,
      signcolumn = "no",
      wrap = true,
    },
  }, popup_options)

  ThreadSummaryView.super.init(self, props, popup_options)

  self._private.namespace = vim.api.nvim_create_namespace("lgtm_summary_" .. self:get_id())
  self._private.line_to_thread = {}
end

function ThreadSummaryView:prop_types()
  return {
    mode = "string",
    threads = "table",
    selected_thread_id = { "string", "nil" },
    on_open_detail = "function",
    on_toggle_collapse = "function",
    on_toggle_all_collapse = "function",
    on_toggle_resolved = "function",
    on_reply = "function",
    on_edit = "function",
    on_delete = "function",
    on_jump = "function",
    on_close = "function",
  }
end

function ThreadSummaryView:mappings()
  local props = self:get_props()

  return {
    {
      mode = { "n" },
      key = "<CR>",
      handler = function()
        local thread = self:get_cursor_thread()
        if thread then
          props.on_open_detail(thread, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "za",
      handler = function()
        local thread = self:get_cursor_thread()
        if thread then
          props.on_toggle_collapse(thread, self)
        end
      end,
    },
    {
      mode = { "n" },
      key = "zA",
      handler = function()
        local thread = self:get_cursor_thread()
        local target = thread and not thread.collapsed or true
        props.on_toggle_all_collapse(target, self)
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

function ThreadSummaryView:on_mount()
  self:_render_buffer()
end

function ThreadSummaryView:on_update()
  self:if_not_mounting_phase(function()
    self:modify_buffer_content(function()
      self:_render_buffer()
    end)
  end)
end

function ThreadSummaryView:on_visibility_change()
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

function ThreadSummaryView:get_cursor_thread()
  local props = self:get_props()
  local threads = props.threads or {}
  if self.winid and vim.api.nvim_win_is_valid(self.winid) then
    local row = vim.api.nvim_win_get_cursor(self.winid)[1]
    local at_row = self._private.line_to_thread[row]
    if at_row then
      return at_row
    end
  end
  local selected_id = props.selected_thread_id
  for _, thread in ipairs(threads) do
    if thread.id == selected_id then
      return thread
    end
  end
  return threads[1]
end

function ThreadSummaryView:get_cursor_comment()
  local thread = self:get_cursor_thread()
  if not thread or not thread.comments then
    return nil
  end
  return thread.comments[#thread.comments]
end

function ThreadSummaryView:get_lines()
  local props = self:get_props()
  return self:_build_summary_lines(props)
end

function ThreadSummaryView.build_lines(props)
  return ThreadSummaryView:_build_summary_lines(props)
end

function ThreadSummaryView:_render_buffer()
  if not self.bufnr or not vim.api.nvim_buf_is_valid(self.bufnr) then
    return
  end
  local lines, line_map = self:get_lines()
  self._private.line_to_thread = line_map
  vim.bo[self.bufnr].filetype = "lgtm-threads-summary"
  self:set_buffer_option("modifiable", true)
  vim.api.nvim_buf_clear_namespace(self.bufnr, self._private.namespace, 0, -1)
  vim.api.nvim_buf_set_lines(self.bufnr, 0, -1, false, {})
  for index, line in ipairs(lines) do
    line:render(self.bufnr, self._private.namespace, index)
  end
  self:set_buffer_option("modifiable", false)
end

function ThreadSummaryView:_build_summary_lines(props)
  local lines = {}
  local line_map = {}
  local threads = props.threads or {}

  for _, header_line in ipairs(self:_pane_header(props.mode, #threads)) do
    table.insert(lines, header_line)
  end

  if #threads == 0 then
    table.insert(lines, self:_empty_line(props.mode))
    return lines, line_map
  end

  for _, thread in ipairs(threads) do
    local is_selected = thread.id == props.selected_thread_id
    for _, item_line in ipairs(self:_thread_card(thread, is_selected)) do
      table.insert(lines, item_line)
      line_map[#lines] = thread
    end
  end

  return lines, line_map
end

function ThreadSummaryView:_pane_header(mode, count)
  local title = mode == "pr" and "PR Review Threads" or "Local Review Threads"
  local header = Line()
  header:append(string.format(" %s (%d)", title, count), "Title")

  local hints = Line()
  hints:append(
    " [<CR>: detail · za/zA: fold · c: reply · x: resolve · gd: jump · q: close]",
    "Comment"
  )

  local divider = Line()
  divider:append(" " .. string.rep("─", 56), "Comment")

  return { header, hints, divider }
end

function ThreadSummaryView:_empty_line(mode)
  local line = Line()
  local text = mode == "pr" and " No active PR threads yet. Press <leader>rc or c on a diff line."
    or " No local review threads yet. Press <leader>rc on any buffer line."
  line:append(text, "Comment")
  return line
end

function ThreadSummaryView:_thread_card(thread, is_selected)
  local rows = {
    self:_thread_header_line(thread, is_selected),
  }
  if not thread.collapsed then
    for _, body_line in ipairs(self:_thread_body_lines(thread)) do
      table.insert(rows, body_line)
    end
  end
  local spacer = Line()
  spacer:append("", "Normal")
  table.insert(rows, spacer)
  return rows
end

function ThreadSummaryView:_thread_header_line(thread, is_selected)
  local root, reply_count = Thread.summary_stats(thread)
  local fold_icon = thread.collapsed and "▸" or "▾"
  local pointer = is_selected and "❯ " or "  "
  local status = thread.resolved and "[RESOLVED]" or "[OPEN]"
  local status_hl = thread.resolved and "DiagnosticOk" or "DiagnosticWarn"

  local line = Line()
  line:append(pointer .. fold_icon .. " ", is_selected and "DiagnosticInfo" or "Comment")
  line:append(status .. " ", status_hl)
  line:append(Thread.format_anchor(thread), "Directory")
  line:append(" · @" .. root.author, "Title")
  if reply_count > 0 then
    line:append(
      string.format(" (%d %s)", reply_count, reply_count == 1 and "reply" or "replies"),
      "Comment"
    )
  end
  return line
end

function ThreadSummaryView:_thread_body_lines(thread)
  local root = thread.comments[1]
  local rows = {}
  local body = root and root.body or ""
  local raw_lines = body ~= "" and vim.split(body, "\n", { plain = true }) or { "" }
  for _, text in ipairs(raw_lines) do
    local line = Line()
    line:append("    " .. text, "Normal")
    table.insert(rows, line)
  end
  return rows
end

return ThreadSummaryView
