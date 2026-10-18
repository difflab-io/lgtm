local M = { history = {} }
function M.show(message, level, previous)
  local notice = vim.notify(message, level or vim.log.levels.INFO, {
    title = "LGTM",
    replace = previous,
    timeout = 5000,
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
