local NuiLine = require("nui.line")
local Thread = require("lgtm.models.thread")

local ThreadSummaryView = {}
local UI = {}
local Private = {}

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

-- Methods ---------------------------------------------------------------------

---Build `NuiLine` rows and line-to-thread map for a summary view state.
---@param props table
---@return NuiLine[], table<integer, LgtmThread>
function ThreadSummaryView.build_lines(props)
  return UI.build_summary_lines(props)
end

-- Private ---------------------------------------------------------------------

function Private.split_lines(text)
  if type(text) ~= "string" or text == "" then
    return { "" }
  end
  return vim.split(text, "\n", { plain = true })
end

return ThreadSummaryView
