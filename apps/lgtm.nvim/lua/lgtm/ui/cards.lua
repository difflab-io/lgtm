local M = {}
local api = vim.api

local function fit(text, width)
  text = text:gsub("\t", "    ")
  if vim.fn.strdisplaywidth(text) > width then
    local prefix = ""
    for index = 0, vim.fn.strchars(text) - 1 do
      local next_char = vim.fn.strcharpart(text, index, 1)
      if vim.fn.strdisplaywidth(prefix .. next_char) > width - 1 then
        break
      end
      prefix = prefix .. next_char
    end
    text = prefix .. "…"
  end
  return text .. string.rep(" ", math.max(0, width - vim.fn.strdisplaywidth(text)))
end

function M.highlights()
  require("lgtm.ui.theme").highlights()
  for name, source in pairs({
    Body = "LgtmNormal",
    Border = "LgtmBorder",
    Status = "LgtmInfo",
    Resolved = "LgtmSuccess",
  }) do
    api.nvim_set_hl(0, "LgtmComment" .. name, { link = source, default = true })
  end
end

-- Collapsed cards keep a single preview row; expansion reveals the complete conversation.
function M.render(card, width, expanded)
  local comments, status = card.comments, card.status
  width = math.max(8, width)
  local border = require("lgtm.ui.theme").border()
  local resolved = card.resolved
  if resolved and not expanded then
    return {
      {
        { fit(" Resolved · Enter open · za expand", width - 2), "LgtmCommentResolved" },
        { "✓ ", "LgtmCommentResolved" },
      },
    }
  end
  local heading = (card.file and "File · " or "")
    .. (status or (resolved and "Resolved" or "Open"))
  if status then
    heading = heading .. (resolved and " · Resolved" or " · Open")
  end
  local fallback = card.location_status
  if fallback then
    local label = card.location_status:sub(1, 1):upper() .. card.location_status:sub(2)
    if card.location_status == "stale" or card.location_status == "outdated" then
      label = label .. " · anchor lost"
    end
    heading = label .. " · " .. heading
  end
  local replies = 0
  for _, comment in ipairs(comments) do
    if comment.is_reply then
      replies = replies + 1
    end
  end
  heading = heading .. " · " .. replies .. (replies == 1 and " reply" or " replies")
  local status_hl = resolved and "LgtmCommentResolved" or "LgtmCommentStatus"
  local rows = {
    {
      { border[1] .. border[2], { "LgtmNormal", "LgtmCommentBorder" } },
      { fit(" " .. heading .. " ", width - 3), { "LgtmNormal", "LgtmTitle", status_hl } },
      { border[3], { "LgtmNormal", "LgtmCommentBorder" } },
    },
  }
  local function content(text)
    rows[#rows + 1] = {
      { border[8] .. " ", { "LgtmNormal", "LgtmCommentBorder" } },
      { fit(text, width - 4), "LgtmCommentBody" },
      { " " .. border[4], { "LgtmNormal", "LgtmCommentBorder" } },
    }
  end
  if expanded then
    for _, comment in ipairs(comments) do
      content(comment.author)
      for _, line in ipairs(vim.split(comment.body, "\n", { plain = true })) do
        if line == "" then
          content("")
        end
        while #line > 0 do
          local length, part = 0, ""
          for index = 0, vim.fn.strchars(line) - 1 do
            local char = vim.fn.strcharpart(line, index, 1)
            if vim.fn.strdisplaywidth(part .. char) > width - 4 then
              break
            end
            part, length = part .. char, index + 1
          end
          if length == 0 then
            part, length = "…", 1
          end
          content(part)
          line = vim.fn.strcharpart(line, length)
        end
      end
    end
  else
    local root = comments[1]
    local preview = root.body:match("[^\r\n]+") or ""
    if root.body:find("\n", 1, true) then
      preview = preview .. " …"
    end
    content(preview)
  end
  rows[#rows + 1] = {
    { border[7] .. border[6], { "LgtmNormal", "LgtmCommentBorder" } },
    {
      fit(
        (not expanded and comments[1].author .. " · " or "")
          .. "Enter open · za "
          .. (expanded and "collapse" or "expand"),
        width - 3
      ),
      { "LgtmNormal", "LgtmCommentBorder" },
    },
    { border[5], { "LgtmNormal", "LgtmCommentBorder" } },
  }
  local Line, Text = require("nui.line"), require("nui.text")
  return vim.tbl_map(function(row)
    local texts = vim.tbl_map(function(chunk)
      return Text(chunk[1], { hl_group = chunk[2] })
    end, row)
    local line = Line(texts)
    if line:width() == 0 then
      return { { "", "LgtmNormal" } }
    end
    return vim.tbl_map(function(text)
      return { text:content(), text.extmark.hl_group }
    end, texts)
  end, rows)
end

return M
