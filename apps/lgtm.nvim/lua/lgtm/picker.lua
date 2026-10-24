-- State -----------------------------------------------------------------------

local M = {}
-- vim.ui.select/input are replaceable by the user's preferred fuzzy picker.
-- Public ----------------------------------------------------------------------

function M.select(title, entries, format, callback)
  if #entries == 0 then
    require("lgtm.notifications").show(title .. ": no results")
    return
  end
  vim.ui.select(entries, { prompt = title, format_item = format }, function(item)
    if item then
      callback(item)
    end
  end)
end

function M.input(title, default, callback)
  vim.ui.input({ prompt = title, default = default }, function(value)
    if value ~= nil then
      callback(value)
    end
  end)
end
return M
