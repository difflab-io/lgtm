local M = {}
local buffer, window, targets
function M.render(review, comments, reviewId, callbacks, open, tab)
  if window and vim.api.nvim_win_is_valid(window) and tab and vim.api.nvim_win_get_tabpage(window) ~= tab then window = nil; buffer = nil end
  if not window or not vim.api.nvim_win_is_valid(window) then
    if not open then return end
    local focus = vim.api.nvim_get_current_win()
    vim.cmd('botright 42vnew')
    window = vim.api.nvim_get_current_win(); buffer = vim.api.nvim_get_current_buf()
    vim.api.nvim_buf_set_name(buffer, 'lgtm-threads://' .. buffer)
    vim.bo[buffer].buftype = 'nofile'; vim.bo[buffer].bufhidden = 'wipe'; vim.bo[buffer].swapfile = false; vim.bo[buffer].filetype = 'markdown'
    vim.api.nvim_set_current_win(focus)
  end
  local lines = { '# LGTM · ' .. (review and (review.kind .. ' · ' .. review.status) or 'Standalone notes'), '', 'c comment · n note · s summary', 'r reply · x resolve · p promote', 'a approve · R request changes', '' }
  targets = {}
  for _, c in ipairs(comments) do
    if not c.threadId and (c.reviewId == reviewId or c.reviewId == '') then
      local start = #lines + 1
      table.insert(lines, '## ' .. c.path .. ':' .. c.line .. ' · ' .. c.side .. (c.resolved and ' [resolved]' or ''))
      table.insert(lines, c.actor.name .. ' [' .. c.actor.kind .. ']' .. (c.reviewId == '' and ' · standalone' or ''))
      for _, line in ipairs(vim.split(c.body, '\n', { plain = true })) do table.insert(lines, line) end
      for _, reply in ipairs(comments) do
        if reply.threadId == c.id then
          table.insert(lines, '↳ ' .. reply.actor.name .. ' [' .. reply.actor.kind .. ']')
          for _, line in ipairs(vim.split(reply.body, '\n', { plain = true })) do table.insert(lines, '  ' .. line) end
        end
      end
      table.insert(lines, '')
      for row = start, #lines do targets[row] = c end
    end
  end
  if #lines == 6 then table.insert(lines, 'No comments yet. Use c in a diff pane.') end
  vim.bo[buffer].modifiable = true
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, lines)
  vim.bo[buffer].modifiable = false
  for key, callback in pairs(callbacks) do vim.keymap.set('n', key, callback, { buffer = buffer, silent = true }) end
end
function M.target()
  return buffer and vim.api.nvim_get_current_buf() == buffer and targets[vim.api.nvim_win_get_cursor(0)[1]] or nil
end
return M
