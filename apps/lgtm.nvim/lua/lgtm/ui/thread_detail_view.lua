local Component = require("nui-components.component")
local NuiLine = require("nui.line")
local Thread = require("lgtm.models.thread")
local fn = require("nui-components.utils.fn")

local UI = {}
local Private = {}

-- State -----------------------------------------------------------------------

local ThreadDetailView = Component:extend("LgtmThreadDetailView")

function ThreadDetailView:init(props, popup_options)
  ThreadDetailView.super.init(
    self,
    fn.merge({
      thread = nil,
      on_back = fn.ignore,
      on_reply = fn.ignore,
      on_edit_comment = fn.ignore,
      on_delete_comment = fn.ignore,
      on_toggle_resolved = fn.ignore,
      on_jump_to_location = fn.ignore,
      on_close = fn.ignore,
    }, props),
    fn.deep_merge({
      buf_options = {
        modifiable = false,
        filetype = "lgtm-thread-detail",
      },
      win_options = {
        cursorline = true,
        wrap = true,
      },
    }, popup_options)
  )
  self.line_to_comment = {}
end

function ThreadDetailView:prop_types()
  return {
    thread = { "table", "nil" },
    on_back = "function",
    on_reply = "function",
    on_edit_comment = "function",
    on_delete_comment = "function",
    on_toggle_resolved = "function",
    on_jump_to_location = "function",
    on_close = "function",
  }
end

-- Render ----------------------------------------------------------------------

function ThreadDetailView:on_update()
  self:render_detail_buffer()
end

function ThreadDetailView:render_detail_buffer()
  if not self.bufnr or not vim.api.nvim_buf_is_valid(self.bufnr) then
    return
  end
  self:modify_buffer_content(function()
    local props = self:get_props()
    local lines, comment_map = UI.build_detail_lines(props.thread)
    self.line_to_comment = comment_map
    Private.write_nui_lines(lines, self.bufnr, self.ns_id)
  end)
end

-- UI --------------------------------------------------------------------------

function UI.build_detail_lines(thread)
  local lines = {}
  local comment_map = {}
  if not thread then
    local empty = NuiLine()
    empty:append(" No thread selected. Press <BS> to return to summary.", "Comment")
    table.insert(lines, empty)
    return lines, comment_map
  end

  for _, header_line in ipairs(UI.detail_header(thread)) do
    table.insert(lines, header_line)
  end

  for index, comment in ipairs(thread.comments or {}) do
    for _, comment_line in ipairs(UI.comment_block(comment, index)) do
      table.insert(lines, comment_line)
      comment_map[#lines] = comment
    end
  end

  return lines, comment_map
end

function UI.detail_header(thread)
  local status = thread.resolved and "[RESOLVED]" or "[OPEN]"
  local status_hl = thread.resolved and "DiagnosticOk" or "DiagnosticWarn"

  local title = NuiLine()
  title:append(" ◀ Thread Detail · ", "Title")
  title:append(status .. " ", status_hl)
  title:append(Thread.format_anchor(thread), "Directory")

  local hints = NuiLine()
  hints:append(
    " [<BS>: summary · c/r: reply · e: edit · dd: delete · x: resolve · gd: jump]",
    "Comment"
  )

  local divider = NuiLine()
  divider:append(" " .. string.rep("─", 56), "Comment")

  return { title, hints, divider }
end

function UI.comment_block(comment, index)
  local header = NuiLine()
  local label = index == 1 and "Root" or string.format("Reply #%d", index - 1)
  header:append(string.format(" %s · @%s ", label, comment.author), "Title")
  header:append("[" .. comment.state .. "]", "Comment")

  local rows = { header }
  for _, text in ipairs(Private.split_lines(comment.body)) do
    local line = NuiLine()
    line:append("   " .. text, "Normal")
    table.insert(rows, line)
  end
  local spacer = NuiLine()
  spacer:append(" " .. string.rep("┄", 40), "Comment")
  table.insert(rows, spacer)
  return rows
end

-- Lifecycle -------------------------------------------------------------------

function ThreadDetailView:mappings()
  return {
    {
      mode = { "n" },
      key = "<BS>",
      handler = function()
        self:get_props().on_back()
      end,
    },
    {
      mode = { "n" },
      key = "h",
      handler = function()
        self:get_props().on_back()
      end,
    },
    {
      mode = { "n" },
      key = "c",
      handler = function()
        local thread = self:get_props().thread
        if thread then
          self:get_props().on_reply(thread)
        end
      end,
    },
    {
      mode = { "n" },
      key = "r",
      handler = function()
        local thread = self:get_props().thread
        if thread then
          self:get_props().on_reply(thread)
        end
      end,
    },
    {
      mode = { "n" },
      key = "e",
      handler = function()
        local thread = self:get_props().thread
        local comment = self:get_comment_at_cursor()
        if thread and comment then
          self:get_props().on_edit_comment(thread, comment)
        end
      end,
    },
    {
      mode = { "n" },
      key = "dd",
      handler = function()
        local thread = self:get_props().thread
        local comment = self:get_comment_at_cursor()
        if thread and comment then
          self:get_props().on_delete_comment(thread, comment)
        end
      end,
    },
    {
      mode = { "n" },
      key = "x",
      handler = function()
        local thread = self:get_props().thread
        if thread then
          self:get_props().on_toggle_resolved(thread)
        end
      end,
    },
    {
      mode = { "n" },
      key = "gd",
      handler = function()
        local thread = self:get_props().thread
        if thread then
          self:get_props().on_jump_to_location(thread)
        end
      end,
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

---Build `NuiLine` rows and line-to-comment map for a thread detail view state.
---@param thread LgtmThread|nil
---@return NuiLine[], table<integer, LgtmComment>
function ThreadDetailView.build_lines(thread)
  return UI.build_detail_lines(thread)
end

---Return the comment under the cursor in the detail view, or the last comment in the thread.
---@return LgtmComment|nil
function ThreadDetailView:get_comment_at_cursor()
  local thread = self:get_props().thread
  if not thread or not thread.comments then
    return nil
  end
  if self.winid and vim.api.nvim_win_is_valid(self.winid) then
    local row = vim.api.nvim_win_get_cursor(self.winid)[1]
    if self.line_to_comment[row] then
      return self.line_to_comment[row]
    end
  end
  return thread.comments[#thread.comments]
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

return ThreadDetailView
