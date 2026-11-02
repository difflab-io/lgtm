local Object = require("nui.object")
local Thread = require("lgtm.models.thread")

local AnchoredThreadSummary = Object("AnchoredThreadSummary")

local highlights_initialized = false

local AUTHOR_PALETTE = {
  "#91d7e3",
  "#7dc4e4",
  "#8bd5ca",
  "#eed49f",
  "#f5a97f",
  "#f5bde6",
  "#c6a0f6",
  "#8aadf4",
}

function AnchoredThreadSummary.ensure_highlights()
  if highlights_initialized then
    return
  end
  highlights_initialized = true

  vim.api.nvim_set_hl(0, "LgtmThreadBorder", { fg = "#89b4fa", default = true })
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadBorderSelected",
    { fg = "#c6a0f6", bold = true, default = true }
  )
  vim.api.nvim_set_hl(0, "LgtmThreadBorderResolved", { fg = "#a6da95", default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignOpen", { fg = "#89b4fa", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignSelected", { fg = "#c6a0f6", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignResolved", { fg = "#a6da95", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadMuted", { fg = "#7f849c", default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadPositive", { fg = "#a6da95", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadWarning", { fg = "#f9e2af", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadInfo", { fg = "#89b4fa", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadBody", { link = "Normal", default = true })

  for idx, hex in ipairs(AUTHOR_PALETTE) do
    vim.api.nvim_set_hl(0, string.format("LgtmAuthorColor%02d", idx), {
      fg = hex,
      bold = true,
      default = true,
    })
  end
end

function AnchoredThreadSummary.border_highlights(thread, is_selected)
  AnchoredThreadSummary.ensure_highlights()
  if is_selected then
    return "LgtmThreadBorderSelected", "LgtmThreadSignSelected"
  end
  if thread.resolved then
    return "LgtmThreadBorderResolved", "LgtmThreadSignResolved"
  end
  return "LgtmThreadBorder", "LgtmThreadSignOpen"
end

function AnchoredThreadSummary:init(thread, opts)
  opts = opts or {}
  local is_selected = opts.is_selected == true
  local border_hl, sign_hl = AnchoredThreadSummary.border_highlights(thread, is_selected)
  local total_width = tonumber(opts.width) or vim.o.columns or 80
  local inner_width = math.max(36, total_width - 4)

  self.thread = thread
  self.is_selected = is_selected
  self.border_hl = border_hl
  self.sign_hl = sign_hl
  self.inner_width = inner_width
  self.content_width = math.max(16, inner_width - 2)
end

function AnchoredThreadSummary:render()
  local virt_lines = {
    self:_top_border_row(),
    self:_box_row(self:_header_chunks()),
  }

  if not self.thread.collapsed then
    for _, row_chunks in ipairs(self:_body_rows()) do
      table.insert(virt_lines, self:_box_row(row_chunks))
    end
    table.insert(virt_lines, self:_separator_row())
    table.insert(virt_lines, self:_box_row(self:_footer_chunks()))
  end

  table.insert(virt_lines, self:_bottom_border_row())
  return virt_lines
end

function AnchoredThreadSummary.render_virt_lines(thread, opts)
  return AnchoredThreadSummary(thread, opts):render()
end

function AnchoredThreadSummary:_top_border_row()
  return {
    { " ╭" .. string.rep("─", self.inner_width) .. "╮", self.border_hl },
  }
end

function AnchoredThreadSummary:_bottom_border_row()
  return {
    { " ╰" .. string.rep("─", self.inner_width) .. "╯", self.border_hl },
  }
end

function AnchoredThreadSummary:_separator_row()
  local rule = string.rep("─", self.content_width)
  return {
    { " │ ", self.border_hl },
    { rule, "LgtmThreadMuted" },
    { " │", self.border_hl },
  }
end

function AnchoredThreadSummary:_box_row(chunks)
  local fitted = self:_fit_chunks(chunks, self.content_width)
  local row = {
    { " │ ", self.border_hl },
  }
  for _, chunk in ipairs(fitted) do
    table.insert(row, chunk)
  end
  table.insert(row, { " │", self.border_hl })
  return row
end

function AnchoredThreadSummary:_header_chunks()
  local root, reply_count = Thread.summary_stats(self.thread)
  local fold_icon = self.thread.collapsed and "▸" or "▾"
  local status_badge = self.thread.resolved and "✓ [RESOLVED]" or "[OPEN]"
  local status_hl = self.thread.resolved and "LgtmThreadPositive" or "LgtmThreadWarning"
  local state_badge = self:_comment_state_badge(root.state)
  local reply_label = self:_reply_label(reply_count)
  local preview = self.thread.collapsed and (" · " .. self:_first_line(root.body)) or ""

  local left = {
    { fold_icon .. " ", "LgtmThreadInfo" },
    { "󰀄 @" .. root.author, self:_author_hl(root.author) },
    { " (" .. Thread.format_anchor(self.thread) .. reply_label .. ")", "LgtmThreadMuted" },
  }
  if preview ~= "" then
    table.insert(left, { preview, "LgtmThreadBody" })
  end

  local right = {
    { state_badge, "LgtmThreadMuted" },
    { status_badge, status_hl },
  }
  return self:_align_left_right(left, right, self.content_width)
end

function AnchoredThreadSummary:_body_rows()
  local rows = {}
  for index, comment in ipairs(self.thread.comments) do
    if index > 1 then
      table.insert(rows, {
        { string.rep("─", self.content_width), "LgtmThreadMuted" },
      })
      table.insert(rows, self:_reply_header_chunks(comment))
    end
    for _, wrapped_line in ipairs(self:_wrap_body(comment.body, self.content_width)) do
      table.insert(rows, {
        { wrapped_line, "LgtmThreadBody" },
      })
    end
  end
  return rows
end

function AnchoredThreadSummary:_reply_header_chunks(comment)
  local left = {
    { "↳ ", "LgtmThreadMuted" },
    { "󰀄 @" .. comment.author, self:_author_hl(comment.author) },
  }
  local right = {
    { "[" .. tostring(comment.state or "DRAFT") .. "]", "LgtmThreadMuted" },
  }
  return self:_align_left_right(left, right, self.content_width)
end

function AnchoredThreadSummary:_footer_chunks()
  return {
    {
      "[<CR>/<leader>ro: detail · <leader>rc: reply · za: fold · x: resolve]",
      "LgtmThreadMuted",
    },
  }
end

function AnchoredThreadSummary:_author_hl(author)
  local name = tostring(author or "you")
  local hash = 0
  for idx = 1, #name do
    hash = (hash * 31 + name:byte(idx)) % #AUTHOR_PALETTE
  end
  return string.format("LgtmAuthorColor%02d", hash + 1)
end

function AnchoredThreadSummary:_comment_state_badge(state)
  if type(state) ~= "string" or state == "" then
    return ""
  end
  return "[" .. state .. "] "
end

function AnchoredThreadSummary:_align_left_right(left_chunks, right_chunks, content_width)
  local right_width = self:_chunks_width(right_chunks)
  local max_left = math.max(4, content_width - right_width - 1)
  local left_fitted = self:_truncate_chunks(left_chunks, max_left)
  local left_width = self:_chunks_width(left_fitted)
  local gap = math.max(1, content_width - left_width - right_width)

  local combined = {}
  for _, chunk in ipairs(left_fitted) do
    table.insert(combined, chunk)
  end
  table.insert(combined, { string.rep(" ", gap), "LgtmThreadMuted" })
  for _, chunk in ipairs(right_chunks) do
    table.insert(combined, chunk)
  end
  return combined
end

function AnchoredThreadSummary:_fit_chunks(chunks, content_width)
  local truncated = self:_truncate_chunks(chunks, content_width)
  local used = self:_chunks_width(truncated)
  if used < content_width then
    table.insert(truncated, { string.rep(" ", content_width - used), "LgtmThreadBody" })
  end
  return truncated
end

function AnchoredThreadSummary:_truncate_chunks(chunks, max_width)
  local out = {}
  local remaining = max_width
  for _, chunk in ipairs(chunks) do
    if remaining <= 0 then
      break
    end
    local text = tostring(chunk[1] or "")
    local hl = chunk[2] or "LgtmThreadBody"
    local w = vim.fn.strdisplaywidth(text)
    if w <= remaining then
      table.insert(out, { text, hl })
      remaining = remaining - w
    else
      local clipped = self:_truncate_text(text, remaining)
      table.insert(out, { clipped, hl })
      remaining = 0
    end
  end
  return out
end

function AnchoredThreadSummary:_truncate_text(text, max_width)
  if max_width <= 0 then
    return ""
  end
  if vim.fn.strdisplaywidth(text) <= max_width then
    return text
  end
  if max_width == 1 then
    return "…"
  end
  local clipped = vim.fn.strcharpart(text, 0, max_width - 1)
  while vim.fn.strdisplaywidth(clipped) > max_width - 1 and #clipped > 0 do
    clipped = vim.fn.strcharpart(clipped, 0, vim.fn.strchars(clipped) - 1)
  end
  return clipped .. "…"
end

function AnchoredThreadSummary:_chunks_width(chunks)
  local total = 0
  for _, chunk in ipairs(chunks) do
    total = total + vim.fn.strdisplaywidth(tostring(chunk[1] or ""))
  end
  return total
end

function AnchoredThreadSummary:_wrap_body(body, max_width)
  local wrapped = {}
  for _, raw_line in ipairs(self:_split_lines(body)) do
    for _, segment in ipairs(self:_wrap_single_line(raw_line, max_width)) do
      table.insert(wrapped, segment)
    end
  end
  return wrapped
end

function AnchoredThreadSummary:_wrap_single_line(line, max_width)
  if line == "" or vim.fn.strdisplaywidth(line) <= max_width then
    return { line }
  end
  local segments = {}
  local rest = line
  while vim.fn.strdisplaywidth(rest) > max_width do
    local part = vim.fn.strcharpart(rest, 0, max_width)
    table.insert(segments, part)
    rest = vim.fn.strcharpart(rest, vim.fn.strchars(part))
  end
  if rest ~= "" then
    table.insert(segments, rest)
  end
  return segments
end

function AnchoredThreadSummary:_reply_label(reply_count)
  if reply_count <= 0 then
    return ""
  end
  if reply_count == 1 then
    return " · 1 reply"
  end
  return string.format(" · %d replies", reply_count)
end

function AnchoredThreadSummary:_first_line(body)
  local lines = self:_split_lines(body)
  return lines[1] or ""
end

function AnchoredThreadSummary:_split_lines(text)
  if type(text) ~= "string" or text == "" then
    return { "" }
  end
  return vim.split(text, "\n", { plain = true })
end

return AnchoredThreadSummary
