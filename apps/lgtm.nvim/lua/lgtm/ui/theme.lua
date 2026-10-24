-- State -----------------------------------------------------------------------

local M = {}
local api = vim.api
M.winhighlight =
  "Normal:LgtmNormal,NormalNC:LgtmNormal,NormalFloat:LgtmNormal,FloatBorder:LgtmBorder,FloatTitle:LgtmTitle,FloatFooter:LgtmMuted,CursorLine:LgtmSelection"
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

-- Public ----------------------------------------------------------------------

function M.highlights()
  for role, source in pairs(require("lgtm.config").options.ui.highlights) do
    if names[role] then
      api.nvim_set_hl(0, names[role], { link = source, default = true })
    end
  end
end

function M.apply(win)
  vim.wo[win].winhighlight = M.winhighlight
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

-- Component props leave border construction to NUI, including custom border tables.
function M.get_component_options()
  return {
    border_style = "none",
    window = {
      highlight = M.winhighlight,
    },
  }
end

function M.get_popup_options(title, footer)
  M.highlights()
  return {
    border = {
      style = require("lgtm.config").options.ui.border,
      text = {
        top = title and (" " .. title .. " "),
        bottom = footer,
      },
    },
    buf_options = { filetype = "lgtm" },
    win_options = {
      winhighlight = M.winhighlight,
      number = false,
      relativenumber = false,
      signcolumn = "no",
      foldcolumn = "0",
      linebreak = true,
    },
  }
end

function M.available()
  for _, module in ipairs({ "nui-components", "nui.split", "nui.tree" }) do
    if not pcall(require, module) then
      vim.notify(
        "LGTM: missing dependency " .. module .. "; see :help lgtm and :checkhealth lgtm",
        vim.log.levels.ERROR
      )
      return false
    end
  end
  return true
end
return M
