local M = {}
-- Atlas-style comment cards use virtual lines below the source anchor.
local function wrapped(text, width)
  local result = {}
  for _, line in ipairs(vim.split(text, '\n', { plain = true })) do
    local chunk = ''
    for index = 0, vim.fn.strchars(line) - 1 do
      local char = vim.fn.strcharpart(line, index, 1)
      if vim.fn.strdisplaywidth(chunk .. char) > width then table.insert(result, chunk); chunk = '' end
      chunk = chunk .. char
    end
    table.insert(result, chunk)
  end
  return result
end
function M.lines(thread, comments, width)
  width = math.max(6, width - 2)
  local lines = {}
  local function row(text, group)
    for _, part in ipairs(wrapped(text, width - 4)) do
      table.insert(lines, { { '│ ', 'LgtmBorder' }, { part .. string.rep(' ', math.max(0, width - 4 - vim.fn.strdisplaywidth(part))), group or 'LgtmComment' }, { ' │', 'LgtmBorder' } })
    end
  end
  table.insert(lines, { { '╭' .. string.rep('─', width - 2) .. '╮', 'LgtmBorder' } })
  row(thread.actor.name .. ' [' .. thread.actor.kind .. '] · ' .. (thread.reviewId == '' and 'standalone' or 'review'), thread.actor.kind == 'agent' and 'LgtmAgent' or 'LgtmHuman')
  row(thread.body)
  for _, reply in ipairs(comments) do
    if reply.threadId == thread.id then
      row(''); row('↳ ' .. reply.actor.name .. ' [' .. reply.actor.kind .. ']', reply.actor.kind == 'agent' and 'LgtmAgent' or 'LgtmHuman'); row(reply.body)
    end
  end
  row('r reply · x resolve · p promote', 'LgtmHint')
  table.insert(lines, { { '╰' .. string.rep('─', width - 2) .. '╯', 'LgtmBorder' } })
  return lines
end
function M.setup()
  for name, opts in pairs({ LgtmBorder = { fg = '#4c566a', bg = '#161b22' }, LgtmComment = { fg = '#d7dbe0', bg = '#161b22' }, LgtmHuman = { fg = '#4c9aff', bg = '#161b22', bold = true }, LgtmAgent = { fg = '#f59f00', bg = '#161b22', bold = true }, LgtmHint = { fg = '#8b94a3', bg = '#161b22' } }) do
    vim.api.nvim_set_hl(0, name, vim.tbl_extend('force', { default = true }, opts))
  end
end
return M
