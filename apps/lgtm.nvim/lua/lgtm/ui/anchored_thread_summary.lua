local Thread = require("lgtm.models.thread")

local AnchoredThreadSummary = {}
local UI = {}
local Private = {}

-- Render ----------------------------------------------------------------------

---Build `virt_lines` extmark tuples for an anchored thread summary in a buffer.
---@param thread LgtmThread
---@param opts { is_selected?: boolean }|nil
---@return table[]
function AnchoredThreadSummary.render_virt_lines(thread, opts)
  opts = opts or {}
  local virt_lines = {
    UI.header_chunks(thread, opts.is_selected == true),
  }
  if not thread.collapsed then
    for _, line_chunks in ipairs(UI.body_chunks(thread)) do
      table.insert(virt_lines, line_chunks)
    end
    table.insert(virt_lines, UI.footer_chunks(thread))
  end
  return virt_lines
end

-- UI --------------------------------------------------------------------------

function UI.header_chunks(thread, is_selected)
  local root, reply_count = Thread.summary_stats(thread)
  local fold_icon = thread.collapsed and "▸" or "▾"
  local status_badge = thread.resolved and "[RESOLVED]" or "[OPEN]"
  local status_hl = thread.resolved and "DiagnosticOk" or "DiagnosticWarn"
  local border_hl = is_selected and "DiagnosticInfo" or "Comment"
  local reply_label = Private.reply_label(reply_count)
  local preview = thread.collapsed and (" · " .. Private.first_line(root.body)) or ""

  return {
    { "  ╭─ " .. fold_icon .. " ", border_hl },
    { status_badge .. " ", status_hl },
    { "@" .. root.author, "Title" },
    { " (" .. Thread.format_anchor(thread) .. reply_label .. ")", "Comment" },
    { preview, "Normal" },
  }
end

function UI.body_chunks(thread)
  local rows = {}
  for index, comment in ipairs(thread.comments) do
    if index > 1 then
      table.insert(rows, {
        { "  │  ↳ ", "Comment" },
        { "@" .. comment.author .. " ", "Special" },
        { "[" .. comment.state .. "]", "Comment" },
      })
    end
    for _, text_line in ipairs(Private.split_lines(comment.body)) do
      table.insert(rows, {
        { "  │  ", "Comment" },
        { text_line, "Normal" },
      })
    end
  end
  return rows
end

function UI.footer_chunks(_thread)
  return {
    {
      "  ╰─ [<CR>/<leader>ro: detail · <leader>rc: reply · za: fold · x: resolve]",
      "Comment",
    },
  }
end

-- Private ---------------------------------------------------------------------

function Private.reply_label(reply_count)
  if reply_count <= 0 then
    return ""
  end
  if reply_count == 1 then
    return " · 1 reply"
  end
  return string.format(" · %d replies", reply_count)
end

function Private.first_line(body)
  local lines = Private.split_lines(body)
  return lines[1] or ""
end

function Private.split_lines(text)
  if type(text) ~= "string" or text == "" then
    return { "" }
  end
  return vim.split(text, "\n", { plain = true })
end

return AnchoredThreadSummary
