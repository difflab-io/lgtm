local M = {}
-- Keep pending text until confirmation, while the editor remains available during the request.
function M.open(title, initial, submit, options)
  local origin = vim.api.nvim_get_current_win()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = "lgtm"
  vim.bo[buf].syntax = "markdown"
  vim.bo[buf].bufhidden = "hide"
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(initial or "", "\n", { plain = true }))
  local width = math.max(20, math.min(80, vim.o.columns - 4))
  local height = math.max(3, math.min(12, vim.o.lines - 5))
  local win = require("lgtm.ui").open(buf, true, {
    relative = "editor",
    width = width,
    height = height,
    row = math.max(0, math.floor((vim.o.lines - height - 2) / 2)),
    col = math.max(0, math.floor((vim.o.columns - width - 2) / 2)),
    border = "rounded",
    title = " " .. title .. " ",
    footer = " <C-s> " .. (options and options.action or "save") .. " · q/Esc cancel ",
  })
  vim.wo[win].number = false
  vim.wo[win].relativenumber = false
  vim.wo[win].signcolumn = "no"
  vim.wo[win].foldcolumn = "0"
  vim.wo[win].wrap = true
  vim.wo[win].linebreak = true
  local pending = false
  local function close()
    local focused = vim.api.nvim_get_current_win() == win
    if vim.api.nvim_win_is_valid(win) then
      vim.api.nvim_win_close(win, true)
    end
    if focused and vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
    end
  end
  local function save()
    if pending then
      return
    end
    pending = true
    vim.api.nvim_win_set_config(win, { hide = true })
    if vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
    end
    local body = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n")
    submit(body, function(err)
      pending = false
      if err then
        if vim.api.nvim_win_is_valid(win) then
          vim.api.nvim_win_set_config(win, { hide = false })
          vim.api.nvim_set_current_win(win)
        end
        require("lgtm.notifications").error(err)
        return
      end
      close()
      if vim.api.nvim_buf_is_valid(buf) then
        vim.api.nvim_buf_delete(buf, { force = true })
      end
    end)
  end
  vim.keymap.set({ "n", "i" }, "<C-s>", save, { buffer = buf, desc = "Save draft" })
  vim.keymap.set("n", "q", close, { buffer = buf })
  vim.keymap.set("n", "<Esc>", close, { buffer = buf })
  M.last = { buf = buf, win = win, save = save }
  return M.last
end
return M
