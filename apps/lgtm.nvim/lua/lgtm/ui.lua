local M = {}
local api = vim.api
local names = {
  normal = "LgtmNormal",
  border = "LgtmBorder",
  title = "LgtmTitle",
  muted = "LgtmMuted",
  selection = "LgtmSelection",
  info = "LgtmInfo",
  success = "LgtmSuccess",
  warning = "LgtmWarning",
  error = "LgtmError",
}

function M.highlights()
  for role, source in pairs(require("lgtm.config").options.ui.highlights) do
    if names[role] then
      api.nvim_set_hl(0, names[role], { link = source, default = true })
    end
  end
end

function M.apply(win)
  vim.wo[win].winhighlight =
    "Normal:LgtmNormal,NormalNC:LgtmNormal,NormalFloat:LgtmNormal,FloatBorder:LgtmBorder,FloatTitle:LgtmTitle,FloatFooter:LgtmMuted,CursorLine:LgtmSelection"
  vim.wo[win].number = false
  vim.wo[win].relativenumber = false
  vim.wo[win].signcolumn = "no"
  vim.wo[win].foldcolumn = "0"
  vim.wo[win].linebreak = true
end

function M.border()
  local style = require("lgtm.config").options.ui.border
  local shapes = {
    rounded = { "╭", "─", "╮", "│", "╯", "─", "╰", "│" },
    single = { "┌", "─", "┐", "│", "┘", "─", "└", "│" },
    double = { "╔", "═", "╗", "║", "╝", "═", "╚", "║" },
    solid = { " ", " ", " ", " ", " ", " ", " ", " " },
    none = { " ", " ", " ", " ", " ", " ", " ", " " },
  }
  if type(style) == "table" and #style == 0 then
    return shapes.none
  end
  if type(style) ~= "table" then
    return shapes[style] or shapes.rounded
  end
  local result = {}
  for index = 1, 8 do
    local part = style[(index - 1) % #style + 1]
    result[index] = type(part) == "table" and part[1] or part
  end
  return result
end

function M.open(buf, enter, options)
  M.highlights()
  options.border = require("lgtm.config").options.ui.border
  local win = api.nvim_open_win(buf, enter, options)
  M.apply(win)
  return win
end
return M
