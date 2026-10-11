local M = {}
function M.open(title, initial, save, opts)
  opts = opts or {}
  local buffer = vim.api.nvim_create_buf(false, true)
  vim.bo[buffer].buftype = 'acwrite'; vim.bo[buffer].bufhidden = 'wipe'; vim.bo[buffer].swapfile = false
  vim.bo[buffer].filetype = 'markdown'
  vim.api.nvim_buf_set_name(buffer, 'lgtm-compose://' .. buffer)
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, vim.split(initial or '', '\n', { plain = true }))
  local width = math.min(80, vim.o.columns - 6)
  local height = math.min(12, vim.o.lines - 6)
  local window = vim.api.nvim_open_win(buffer, true, { relative = 'editor', width = width, height = height,
    row = math.floor((vim.o.lines - height) / 2), col = math.floor((vim.o.columns - width) / 2),
    style = 'minimal', border = 'rounded', title = ' ' .. title .. ' ', footer = ' Ctrl-S / :w save · q cancel ' })
  local function close() if vim.api.nvim_buf_is_valid(buffer) then vim.bo[buffer].modified = false end; if vim.api.nvim_win_is_valid(window) then vim.api.nvim_win_close(window, true) end end
  local submitting = false
  local function submit()
    if submitting then return end
    local body = table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), '\n')
    if body:match('^%s*$') and not opts.allow_empty then vim.notify('Enter a nonempty comment or summary', vim.log.levels.WARN); return end
    submitting = true
    save(body, close, function()
      submitting = false
      if vim.api.nvim_win_is_valid(window) then vim.api.nvim_set_current_win(window) end
    end)
  end
  vim.api.nvim_create_autocmd('BufWriteCmd', { buffer = buffer, callback = submit })
  vim.keymap.set({ 'n', 'i' }, '<C-s>', submit, { buffer = buffer, desc = 'Save LGTM comment' })
  vim.keymap.set({ 'n', 'i' }, '<D-CR>', submit, { buffer = buffer })
  vim.keymap.set('n', 'q', close, { buffer = buffer })
  vim.keymap.set('n', '<Esc>', close, { buffer = buffer })
  vim.cmd('startinsert')
end
return M
