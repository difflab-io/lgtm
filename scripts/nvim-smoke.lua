vim.opt.runtimepath:append(vim.fn.getcwd() .. '/libs/lgtm-nvim')
local lgtm = require('lgtm')
lgtm.setup({ actor = { name = 'Codex', kind = 'agent' } })
for _, command in ipairs({ 'Lgtm', 'LgtmNew', 'LgtmDiff', 'LgtmComment', 'LgtmNote', 'LgtmPromote', 'LgtmSummary', 'LgtmSubmit', 'LgtmApprove', 'LgtmReject' }) do
  assert(vim.fn.exists(':' .. command) == 2, command .. ' missing')
end
print('Neovim plugin loaded; review commands registered')
vim.cmd('qa!')
