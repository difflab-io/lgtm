-- State -----------------------------------------------------------------------

local M = { history = {} }
local theme_namespace = vim.api.nvim_create_namespace("lgtm.notification-theme")

-- Methods ---------------------------------------------------------------------

function M.show(message, level, previous)
  local notice = require("notify")(message, level or vim.log.levels.INFO, {
    title = "LGTM",
    replace = previous,
    timeout = 5000,
    render = function(buf, notification, highlights, config)
      require("lgtm.ui.theme").highlights()
      local accent = notification.level == "ERROR" and "LgtmError"
        or notification.level == "WARN" and "LgtmWarning"
        or "LgtmInfo"
      -- The documented renderer supplies the provider's per-notification groups.
      -- Window-local definitions do not change other plugins' notifications.
      for section, name in pairs({
        body = "LgtmNormal",
        border = accent,
        title = accent,
        icon = accent,
      }) do
        vim.api.nvim_set_hl(theme_namespace, highlights[section], { link = name })
      end
      require("notify.render.compact")(buf, notification, highlights, config)
    end,
    on_open = function(win)
      vim.api.nvim_win_set_hl_ns(win, theme_namespace)
      vim.api.nvim_win_set_config(win, { border = require("lgtm.config").options.ui.border })
    end,
  })
  M.history[#M.history + 1] = { message = message, level = level }
  return notice
end

function M.error(err, previous)
  return M.show(
    type(err) == "table" and (err.message or err.code) or tostring(err),
    vim.log.levels.ERROR,
    previous
  )
end
return M
