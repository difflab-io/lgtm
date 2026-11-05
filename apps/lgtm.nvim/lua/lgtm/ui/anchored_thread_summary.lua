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

local CARD_BG = "#181d2b"
local CARD_HEADER_BG = "#202635"

function AnchoredThreadSummary.ensure_highlights()
  if highlights_initialized then
    return
  end
  highlights_initialized = true

  vim.api.nvim_set_hl(0, "LgtmThreadBorder", { fg = "#89b4fa", bg = CARD_BG, default = true })
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadBorderSelected",
    { fg = "#c6a0f6", bg = CARD_BG, bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadBorderResolved",
    { fg = "#86efac", bg = CARD_BG, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadBorderOutdated",
    { fg = "#f5a97f", bg = CARD_BG, default = true }
  )

  vim.api.nvim_set_hl(0, "LgtmThreadSignOpen", { fg = "#89b4fa", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignSelected", { fg = "#c6a0f6", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignResolved", { fg = "#86efac", bold = true, default = true })
  vim.api.nvim_set_hl(0, "LgtmThreadSignOutdated", { fg = "#f5a97f", bold = true, default = true })

  vim.api.nvim_set_hl(0, "LgtmThreadBody", { fg = "#cdd6f4", bg = CARD_BG, default = true })
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadHeaderBg",
    { fg = "#cdd6f4", bg = CARD_HEADER_BG, default = true }
  )
  vim.api.nvim_set_hl(0, "LgtmThreadMuted", { fg = "#7f849c", bg = CARD_BG, default = true })
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadHeaderMuted",
    { fg = "#7f849c", bg = CARD_HEADER_BG, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadPositive",
    { fg = "#86efac", bg = CARD_HEADER_BG, bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadWarning",
    { fg = "#fcd34d", bg = CARD_HEADER_BG, bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmThreadInfo",
    { fg = "#89b4fa", bg = CARD_HEADER_BG, bold = true, default = true }
  )

  vim.api.nvim_set_hl(
    0,
    "LgtmBadgeDraft",
    { fg = "#111827", bg = "#fcd34d", bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmBadgePending",
    { fg = "#0b1320", bg = "#89b4fa", bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmBadgePublished",
    { fg = "#cdd6f4", bg = "#313244", bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmBadgeOpen",
    { fg = "#0b1320", bg = "#7dc4e4", bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmBadgeResolved",
    { fg = "#0b1320", bg = "#86efac", bold = true, default = true }
  )
  vim.api.nvim_set_hl(
    0,
    "LgtmBadgeOutdated",
    { fg = "#111827", bg = "#f5a97f", bold = true, default = true }
  )

  for idx, hex in ipairs(AUTHOR_PALETTE) do
    vim.api.nvim_set_hl(0, string.format("LgtmAuthorColor%02d", idx), {
      fg = hex,
      bg = CARD_HEADER_BG,
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
  if thread.outdated then
    return "LgtmThreadBorderOutdated", "LgtmThreadSignOutdated"
  end
  if thread.resolved then
    return "LgtmThreadBorderResolved", "LgtmThreadSignResolved"
  end
  return "LgtmThreadBorder", "LgtmThreadSignOpen"
end

function AnchoredThreadSummary.state_badge_spec(state)
  AnchoredThreadSummary.ensure_highlights()
  local normalized = tostring(state or "DRAFT"):upper()
  if normalized == "PENDING" then
    return " 󰦖 PENDING ", "LgtmBadgePending"
  end
  if normalized == "PUBLISHED" then
    return "  PUBLISHED ", "LgtmBadgePublished"
  end
  return "  DRAFT ", "LgtmBadgeDraft"
end

function AnchoredThreadSummary.thread_status_badges(thread)
  AnchoredThreadSummary.ensure_highlights()
  local badges = {}
  if thread.outdated then
    table.insert(badges, { " 󰦖 STALE ", "LgtmBadgeOutdated" })
  end
  if thread.resolved then
    table.insert(badges, { "  RESOLVED ", "LgtmBadgeResolved" })
  else
    table.insert(badges, { " ● OPEN ", "LgtmBadgeOpen" })
  end
  return badges
end

function AnchoredThreadSummary:init(thread, opts)
  opts = opts or {}
  local is_selected = opts.is_selected == true
  local border_hl, sign_hl = AnchoredThreadSummary.border_highlights(thread, is_selected)
  local total_width = math.max(20, tonumber(opts.width) or vim.o.columns or 80)
  local inner_width = math.max(18, total_width - 2)

  self.thread = thread
  self.is_selected = is_selected
  self.border_hl = border_hl
  self.sign_hl = sign_hl
  self.total_width = total_width
  self.inner_width = inner_width
  self.content_width = math.max(14, inner_width - 2)
end

function AnchoredThreadSummary:render()
  local virt_lines = {
    self:_top_border_row(),
    self:_box_row(self:_header_chunks(), "LgtmThreadHeaderBg"),
  }

  if not self.thread.collapsed then
    for _, row_spec in ipairs(self:_body_rows()) do
      table.insert(virt_lines, self:_box_row(row_spec.chunks, row_spec.fill_hl))
    end
    table.insert(virt_lines, self:_separator_row())
    table.insert(virt_lines, self:_box_row(self:_footer_chunks(), "LgtmThreadHeaderBg"))
  end

  table.insert(virt_lines, self:_bottom_border_row())
  return virt_lines
end

function AnchoredThreadSummary.render_virt_lines(thread, opts)
  return AnchoredThreadSummary(thread, opts):render()
end

function AnchoredThreadSummary:_top_border_row()
  return {
    { "╭" .. string.rep("─", self.inner_width) .. "╮", self.border_hl },
  }
end

function AnchoredThreadSummary:_bottom_border_row()
  return {
    { "╰" .. string.rep("─", self.inner_width) .. "╯", self.border_hl },
  }
end

function AnchoredThreadSummary:_separator_row()
  local rule = string.rep("─", self.content_width)
  return {
    { "│", self.border_hl },
    { " " .. rule .. " ", "LgtmThreadMuted" },
    { "│", self.border_hl },
  }
end

function AnchoredThreadSummary:_box_row(chunks, fill_hl)
  local bg_hl = fill_hl or "LgtmThreadBody"
  local fitted = self:_fit_chunks(chunks, self.content_width, bg_hl)
  local row = {
    { "│", self.border_hl },
    { " ", bg_hl },
  }
  for _, chunk in ipairs(fitted) do
    table.insert(row, chunk)
  end
  table.insert(row, { " ", bg_hl })
  table.insert(row, { "│", self.border_hl })
  return row
end

function AnchoredThreadSummary:_header_chunks()
  local root, reply_count = Thread.summary_stats(self.thread)
  local fold_icon = self.thread.collapsed and "" or ""
  local state_label, state_hl = AnchoredThreadSummary.state_badge_spec(root.state)
  local reply_label = self:_reply_label(reply_count)
  local preview = self.thread.collapsed and (" · " .. self:_first_line(root.body)) or ""

  local left = {
    { fold_icon .. " ", "LgtmThreadInfo" },
    { " @" .. root.author, self:_author_hl(root.author) },
    { " (" .. Thread.format_anchor(self.thread) .. reply_label .. ")", "LgtmThreadHeaderMuted" },
  }
  if preview ~= "" then
    table.insert(left, { preview, "LgtmThreadHeaderBg" })
  end

  local right = {
    { state_label, state_hl },
  }
  for _, badge in ipairs(AnchoredThreadSummary.thread_status_badges(self.thread)) do
    table.insert(right, { " ", "LgtmThreadHeaderBg" })
    table.insert(right, badge)
  end

  return self:_align_left_right(left, right, self.content_width, "LgtmThreadHeaderBg")
end

function AnchoredThreadSummary:_body_rows()
  local rows = {}
  for index, comment in ipairs(self.thread.comments) do
    if index > 1 then
      table.insert(rows, {
        fill_hl = "LgtmThreadBody",
        chunks = {
          { string.rep("─", self.content_width), "LgtmThreadMuted" },
        },
      })
      table.insert(rows, {
        fill_hl = "LgtmThreadHeaderBg",
        chunks = self:_reply_header_chunks(comment),
      })
    end
    for _, wrapped_line in ipairs(self:_wrap_body(comment.body, self.content_width)) do
      table.insert(rows, {
        fill_hl = "LgtmThreadBody",
        chunks = {
          { wrapped_line, "LgtmThreadBody" },
        },
      })
    end
  end
  return rows
end

function AnchoredThreadSummary:_reply_header_chunks(comment)
  local state_label, state_hl = AnchoredThreadSummary.state_badge_spec(comment.state)
  local left = {
    { " ", "LgtmThreadHeaderMuted" },
    { " @" .. comment.author, self:_author_hl(comment.author) },
  }
  local right = {
    { state_label, state_hl },
  }
  return self:_align_left_right(left, right, self.content_width, "LgtmThreadHeaderBg")
end

function AnchoredThreadSummary:_footer_chunks()
  return {
    {
      "<CR>/<leader>ro detail   <leader>rc reply   za fold   x resolve",
      "LgtmThreadHeaderMuted",
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

function AnchoredThreadSummary:_align_left_right(left_chunks, right_chunks, content_width, gap_hl)
  local right_width = self:_chunks_width(right_chunks)
  local max_left = math.max(4, content_width - right_width - 1)
  local left_fitted = self:_truncate_chunks(left_chunks, max_left)
  local left_width = self:_chunks_width(left_fitted)
  local gap = math.max(1, content_width - left_width - right_width)

  local combined = {}
  for _, chunk in ipairs(left_fitted) do
    table.insert(combined, chunk)
  end
  table.insert(combined, { string.rep(" ", gap), gap_hl or "LgtmThreadHeaderMuted" })
  for _, chunk in ipairs(right_chunks) do
    table.insert(combined, chunk)
  end
  return combined
end

function AnchoredThreadSummary:_fit_chunks(chunks, content_width, fill_hl)
  local truncated = self:_truncate_chunks(chunks, content_width)
  local used = self:_chunks_width(truncated)
  if used < content_width then
    table.insert(truncated, { string.rep(" ", content_width - used), fill_hl or "LgtmThreadBody" })
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
