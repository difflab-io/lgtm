-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local border_highlight = { "LgtmNormal", "LgtmCommentBorder" }

-- Public ----------------------------------------------------------------------

function M.highlights()
  require("lgtm.ui.theme").highlights()
  for name, source in pairs({
    Body = "LgtmNormal",
    Border = "LgtmBorder",
    Status = "LgtmInfo",
    Resolved = "LgtmSuccess",
  }) do
    vim.api.nvim_set_hl(0, "LgtmComment" .. name, { link = source, default = true })
  end
end

-- Render ----------------------------------------------------------------------

function M.render(card, width, expanded)
  width = math.max(8, width)
  if card.resolved and not expanded then
    return Private.to_virtual_lines({
      Private.resolved_summary(width),
    })
  end

  return Private.to_virtual_lines({
    Private.header(card, width),
    Private.body(card, width, expanded),
    Private.footer(card, width, expanded),
  })
end

-- Private ---------------------------------------------------------------------

function Private.header(card, width)
  local Text = require("nui.text")
  local border = require("lgtm.ui.theme").border()
  local status = card.resolved and "LgtmCommentResolved" or "LgtmCommentStatus"
  return {
    {
      Text(border[1] .. border[2], { hl_group = border_highlight }),
      Text(Private.fit(" " .. Private.heading(card) .. " ", width - 3), {
        hl_group = { "LgtmNormal", "LgtmTitle", status },
      }),
      Text(border[3], { hl_group = border_highlight }),
    },
  }
end

function Private.body(card, width, expanded)
  if expanded then
    return Private.conversation(card.comments, width)
  end
  return {
    Private.content(Private.preview(card.comments[1].body), width),
  }
end

function Private.footer(card, width, expanded)
  local Text = require("nui.text")
  local border = require("lgtm.ui.theme").border()
  local author = not expanded and card.comments[1].author .. " · " or ""
  local action = expanded and "collapse" or "expand"
  return {
    {
      Text(border[7] .. border[6], { hl_group = border_highlight }),
      Text(Private.fit(author .. "Enter open · za " .. action, width - 3), {
        hl_group = border_highlight,
      }),
      Text(border[5], { hl_group = border_highlight }),
    },
  }
end

function Private.resolved_summary(width)
  local Text = require("nui.text")
  return {
    {
      Text(Private.fit(" Resolved · Enter open · za expand", width - 2), {
        hl_group = "LgtmCommentResolved",
      }),
      Text("✓ ", { hl_group = "LgtmCommentResolved" }),
    },
  }
end

function Private.conversation(comments, width)
  local rows = {}
  for _, comment in ipairs(comments) do
    rows[#rows + 1] = Private.content(comment.author, width)
    for _, text in ipairs(Private.wrap(comment.body, width - 4)) do
      rows[#rows + 1] = Private.content(text, width)
    end
  end
  return rows
end

function Private.content(text, width)
  local Text = require("nui.text")
  local border = require("lgtm.ui.theme").border()
  return {
    Text(border[8] .. " ", { hl_group = border_highlight }),
    Text(Private.fit(text, width - 4), { hl_group = "LgtmCommentBody" }),
    Text(" " .. border[4], { hl_group = border_highlight }),
  }
end

function Private.heading(card)
  local status = card.resolved and "Resolved" or "Open"
  if card.status then
    status = card.status .. " · " .. status
  end
  if card.file then
    status = "File · " .. status
  end
  local location = Private.location_label(card.location_status)
  if location then
    status = location .. " · " .. status
  end
  local replies = #vim.tbl_filter(function(comment)
    return comment.is_reply
  end, card.comments)
  local suffix = replies == 1 and " reply" or " replies"
  return status .. " · " .. replies .. suffix
end

function Private.location_label(status)
  if not status then
    return
  end
  local label = status:sub(1, 1):upper() .. status:sub(2)
  if status == "stale" or status == "outdated" then
    label = label .. " · anchor lost"
  end
  return label
end

function Private.preview(body)
  local preview = body:match("[^\r\n]+") or ""
  if body:find("\n", 1, true) then
    preview = preview .. " …"
  end
  return preview
end

function Private.to_virtual_lines(sections)
  local rows = {}
  for _, section in ipairs(sections) do
    for _, texts in ipairs(section) do
      rows[#rows + 1] = Private.to_virtual_line(texts)
    end
  end
  return rows
end

function Private.to_virtual_line(texts)
  local line = require("nui.line")(texts)
  if line:width() == 0 then
    return { { "", "LgtmNormal" } }
  end
  return vim.tbl_map(function(text)
    return { text:content(), text.extmark.hl_group }
  end, texts)
end

function Private.wrap(body, width)
  local rows = {}
  for _, line in ipairs(vim.split(body, "\n", { plain = true })) do
    if line == "" then
      rows[#rows + 1] = ""
    end
    while #line > 0 do
      local part, length = Private.take_line(line, width)
      rows[#rows + 1] = part
      line = vim.fn.strcharpart(line, length)
    end
  end
  return rows
end

function Private.fit(text, width)
  text = text:gsub("\t", "    ")
  if vim.fn.strdisplaywidth(text) > width then
    local prefix = Private.take_line(text, width - 1)
    text = prefix .. "…"
  end
  return text .. string.rep(" ", math.max(0, width - vim.fn.strdisplaywidth(text)))
end

function Private.take_line(text, width)
  local part, length = "", 0
  for index = 0, vim.fn.strchars(text) - 1 do
    local char = vim.fn.strcharpart(text, index, 1)
    if vim.fn.strdisplaywidth(part .. char) > width then
      break
    end
    part, length = part .. char, index + 1
  end
  if length == 0 then
    return "…", 1
  end
  return part, length
end

return M
