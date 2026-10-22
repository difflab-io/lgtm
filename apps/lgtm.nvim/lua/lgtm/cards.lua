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
  require("lgtm.ui").highlights()
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
function M.render(thread, comments, status, width, expanded, placement)
  width = math.max(8, width)
  local border = require("lgtm.ui").border()
  local resolved = thread.state == "resolved"
  if resolved and not expanded then
    return {
      {
        { fit(" Resolved · Enter open · za expand", width - 2), "LgtmCommentResolved" },
        { "✓ ", "LgtmCommentResolved" },
      },
    }
  end
  local heading = (thread.anchor.kind == "file" and "File · " or "")
    .. (status or (resolved and "Resolved" or "Open"))
  if status then
    heading = heading .. (resolved and " · Resolved" or " · Open")
  end
  local fallback = placement and not placement.location
  if fallback then
    heading = placement.status:sub(1, 1):upper() .. placement.status:sub(2) .. " · " .. heading
  end
  local replies = 0
  for _, comment in ipairs(comments) do
    if tonumber(comment.position or 0) > 0 then
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
  if fallback then
    local original = thread.anchor
    if original.start then
      content(
        "Original lines " .. original.start .. "–" .. original["end"] .. " · location lost"
      )
    end
    content(placement.reason or "Original location unavailable")
    if
      original.evidence
      and original.evidence.selectedText
      and original.evidence.selectedText ~= ""
    then
      content("> " .. (original.evidence.selectedText:match("[^\n]+") or ""))
    end
  end
  if expanded then
    for _, comment in ipairs(comments) do
      content(comment.author and comment.author.name or "Unknown")
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
        (
          not expanded and (comments[1].author and comments[1].author.name or "Unknown") .. " · "
          or ""
        )
          .. "Enter open · za "
          .. (expanded and "collapse" or "expand"),
        width - 3
      ),
      { "LgtmNormal", "LgtmCommentBorder" },
    },
    { border[5], { "LgtmNormal", "LgtmCommentBorder" } },
  }
  return rows
end

-- Translate an annotation boundary through CodeDiff's actual edit ranges. A comment
-- inside an insertion/deletion pads the other pane at that edit's adjacent real line.
function M.other_line(line, source, changes, target_count)
  if line == 0 then
    return 0
  end
  local target = source == "old" and "modified" or "original"
  source = source == "old" and "original" or "modified"
  local delta = 0
  for _, change in ipairs(changes or {}) do
    local from, to = change[source], change[target]
    if line < from.start_line then
      break
    end
    if line < from.end_line then
      local mapped = to.start_line - 1
      if to.end_line > to.start_line then
        mapped = to.start_line + math.min(line - from.start_line, to.end_line - to.start_line - 1)
      end
      return math.max(0, math.min(mapped, target_count))
    end
    delta = to.end_line - from.end_line
  end
  return math.max(0, math.min(line + delta, target_count))
end
return M
