local Component = require("nui-components.component")
local NuiLine = require("nui.line")
local Thread = require("lgtm.models.thread")
local fn = require("nui-components.utils.fn")

local UI = {}
local Private = {}

-- State -----------------------------------------------------------------------

local ThreadSummaryView = Component:extend("LgtmThreadSummaryView")

function ThreadSummaryView:init(props, popup_options)
  ThreadSummaryView.super.init(
    self,
    fn.merge({
      mode = "local",
      threads = {},
      selected_thread_id = nil,
      on_select = fn.ignore,
      on_enter_detail = fn.ignore,
      on_toggle_collapse = fn.ignore,
      on_set_all_collapsed = fn.ignore,
      on_toggle_resolved = fn.ignore,
      on_reply = fn.ignore,
      on_jump_to_location = fn.ignore,
      on_close = fn.ignore,
    }, props),
    fn.deep_merge({
      buf_options = {
        modifiable = false,
        filetype = "lgtm-threads-summary",
      },
      win_options = {
        cursorline = true,
        wrap = true,
      },
    }, popup_options)
  )
  self.line_to_thread = {}
end

function ThreadSummaryView:prop_types()
  return {
    mode = "string",
    threads = "table",
    selected_thread_id = { "string", "nil" },
    on_select = "function",
    on_enter_detail = "function",
    on_toggle_collapse = "function",
    on_set_all_collapsed = "function",
    on_toggle_resolved = "function",
    on_reply = "function",
    on_jump_to_location = "function",
    on_close = "function",
  }
end

-- Render ----------------------------------------------------------------------

function ThreadSummaryView:on_update()
  self:render_summary_buffer()
end

function ThreadSummaryView:render_summary_buffer()
  if not self.bufnr or not vim.api.nvim_buf_is_valid(self.bufnr) then
    return
  end
  self:modify_buffer_content(function()
    local props = self:get_props()
    local lines, line_map = UI.build_summary_lines(props)
    self.line_to_thread = line_map
    Private.write_nui_lines(lines, self.bufnr, self.ns_id)
  end)
end

-- UI --------------------------------------------------------------------------

function UI.build_summary_lines(props)
  local lines = {}
  local line_map = {}
  for _, header_line in ipairs(UI.pane_header(props.mode, #props.threads)) do
    table.insert(lines, header_line)
  end

  if #props.threads == 0 then
    table.insert(lines, UI.empty_line(props.mode))
    return lines, line_map
  end

  for _, thread in ipairs(props.threads) do
    local is_selected = thread.id == props.selected_thread_id
    for _, item_line in ipairs(UI.thread_card(thread, is_selected)) do
      table.insert(lines, item_line)
      line_map[#lines] = thread
    end
  end

  return lines, line_map
end

function UI.pane_header(mode, count)
  local title = mode == "pr" and "PR Review Threads" or "Local Review Threads"
  local header = NuiLine()
  header:append(string.format(" %s (%d)", title, count), "Title")

  local hints = NuiLine()
  hints:append(
    " [<CR>: detail · za/zA: fold · c: reply · x: resolve · gd: jump · q: close]",
    "Comment"
  )

  local divider = NuiLine()
  divider:append(" " .. string.rep("─", 56), "Comment")

  return { header, hints, divider }
end

function UI.empty_line(mode)
  local line = NuiLine()
  local text = mode == "pr" and " No active PR threads yet. Press <leader>rc or c on a diff line."
    or " No local review threads yet. Press <leader>rc on any buffer line."
  line:append(text, "Comment")
  return line
end

function UI.thread_card(thread, is_selected)
  local rows = {
    UI.thread_header_line(thread, is_selected),
  }
  if not thread.collapsed then
    for _, body_line in ipairs(UI.thread_body_lines(thread)) do
      table.insert(rows, body_line)
    end
  end
  local spacer = NuiLine()
  spacer:append("", "Normal")
  table.insert(rows, spacer)
  return rows
end

function UI.thread_header_line(thread, is_selected)
  local root, reply_count = Thread.summary_stats(thread)
  local fold_icon = thread.collapsed and "▸" or "▾"
  local pointer = is_selected and "❯ " or "  "
  local status = thread.resolved and "[RESOLVED]" or "[OPEN]"
  local status_hl = thread.resolved and "DiagnosticOk" or "DiagnosticWarn"

  local line = NuiLine()
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

function UI.thread_body_lines(thread)
  local root = thread.comments[1]
  local rows = {}
  for _, text in ipairs(Private.split_lines(root and root.body or "")) do
    local line = NuiLine()
    line:append("    " .. text, "Normal")
    table.insert(rows, line)
  end
  return rows
end

-- Lifecycle -------------------------------------------------------------------

function ThreadSummaryView:mappings()
  local function with_cursor_thread(callback)
    return function()
      local thread = self:get_thread_at_cursor()
      if thread then
        callback(thread)
      end
    end
  end

  return {
    {
      mode = { "n" },
      key = "<CR>",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_enter_detail(thread)
      end),
    },
    {
      mode = { "n" },
      key = "za",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_toggle_collapse(thread)
      end),
    },
    {
      mode = { "n" },
      key = "zA",
      handler = function()
        local thread = self:get_thread_at_cursor()
        local target_collapsed = thread and not thread.collapsed or true
        self:get_props().on_set_all_collapsed(target_collapsed)
      end,
    },
    {
      mode = { "n" },
      key = "x",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_toggle_resolved(thread)
      end),
    },
    {
      mode = { "n" },
      key = "c",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_reply(thread)
      end),
    },
    {
      mode = { "n" },
      key = "r",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_reply(thread)
      end),
    },
    {
      mode = { "n" },
      key = "gd",
      handler = with_cursor_thread(function(thread)
        self:get_props().on_jump_to_location(thread)
      end),
    },
    {
      mode = { "n" },
      key = "q",
      handler = function()
        self:get_props().on_close()
      end,
    },
  }
end

-- Methods ---------------------------------------------------------------------

---Build `NuiLine` rows and line-to-thread map for a summary view state.
---@param props table
---@return NuiLine[], table<integer, LgtmThread>
function ThreadSummaryView.build_lines(props)
  return UI.build_summary_lines(props)
end

---Return the thread corresponding to the current cursor line in the summary view.
---@return LgtmThread|nil
function ThreadSummaryView:get_thread_at_cursor()
  local props = self:get_props()
  if self.winid and vim.api.nvim_win_is_valid(self.winid) then
    local row = vim.api.nvim_win_get_cursor(self.winid)[1]
    if self.line_to_thread[row] then
      return self.line_to_thread[row]
    end
  end
  for _, thread in ipairs(props.threads or {}) do
    if thread.id == props.selected_thread_id then
      return thread
    end
  end
  return (props.threads or {})[1]
end

-- Private ---------------------------------------------------------------------

function Private.split_lines(text)
  if type(text) ~= "string" or text == "" then
    return { "" }
  end
  return vim.split(text, "\n", { plain = true })
end

function Private.write_nui_lines(lines, bufnr, ns_id)
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, {})
  for index, line in ipairs(lines) do
    line:render(bufnr, ns_id, index)
  end
end

return ThreadSummaryView
