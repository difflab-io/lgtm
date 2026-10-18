local M = {}
-- Keep failed drafts in the same buffer. Confirmation alone closes the composer.
function M.open(title, initial, submit)
  local origin = vim.api.nvim_get_current_win()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = "lgtm"
  vim.bo[buf].syntax = "markdown"
  vim.bo[buf].bufhidden = "hide"
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(initial or "", "\n", { plain = true }))
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = math.max(20, math.min(80, vim.o.columns - 4)),
    height = math.max(3, math.min(12, vim.o.lines - 5)),
    row = 2,
    col = 2,
    border = "rounded",
    title = " " .. title .. " ",
    footer = " <C-s> save · q/Esc cancel ",
  })
  local pending = false
  local function close()
    if vim.api.nvim_win_is_valid(win) then
      vim.api.nvim_win_close(win, true)
    end
    if vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
    end
  end
  local function save()
    if pending then
      return
    end
    pending = true
    local body = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n")
    submit(body, function(err)
      pending = false
      if err then
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
