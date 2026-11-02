local NuiLine = require("nui.line")
local Thread = require("lgtm.models.thread")

local ThreadDetailView = {}
local UI = {}
local Private = {}

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

-- Methods ---------------------------------------------------------------------

---Build `NuiLine` rows and line-to-comment map for a thread detail view state.
---@param thread LgtmThread|nil
---@return NuiLine[], table<integer, LgtmComment>
function ThreadDetailView.build_lines(thread)
  return UI.build_detail_lines(thread)
end

-- Private ---------------------------------------------------------------------

function Private.split_lines(text)
  if type(text) ~= "string" or text == "" then
    return { "" }
  end
  return vim.split(text, "\n", { plain = true })
end

return ThreadDetailView
