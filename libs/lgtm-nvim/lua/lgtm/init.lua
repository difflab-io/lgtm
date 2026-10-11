local M = {}
local service = require('lgtm.service')
local diff = require('lgtm.codediff')
local compose = require('lgtm.compose')
local panel = require('lgtm.panel')
local project = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h:h:h')
local config = { project = project, actor = { name = vim.env.USER or 'me', kind = 'human' } }
local context, selected, tab = nil, '', nil
local snapshot = { reviews = {}, comments = {} }
local opening = false
local function current_review()
  for _, r in ipairs(snapshot.reviews) do if r.id == selected then return r end end
end
local function request(payload, callback, onerror)
  service.request(context, payload, function(out) snapshot = out.state; if callback then callback(out) end end, onerror)
end
local function keys(for_diff)
  local mappings = { c = function() M.comment(false) end, n = function() M.comment(true) end,
    ['<leader>lc'] = function() M.comment(false) end, ['<leader>ln'] = function() M.comment(true) end,
    ['<leader>lt'] = M.threads, ['<leader>ls'] = M.summary, ['<leader>la'] = function() M.submit('APPROVE') end,
    ['<leader>lr'] = function() M.submit('REQUEST_CHANGES') end,
    r = M.reply, x = M.resolve, p = M.promote, s = M.summary, a = function() M.submit('APPROVE') end,
    R = function() M.submit('REQUEST_CHANGES') end }
  if for_diff then for _, key in ipairs({ 's', 'a', 'R' }) do mappings[key] = nil end end
  return mappings
end
function M.refresh(openPanel)
  request({ op = 'state' }, function()
    if tab and vim.api.nvim_get_current_tabpage() ~= tab then return end
    if tab and vim.api.nvim_tabpage_is_valid(tab) then diff.mark(tab, snapshot.comments, selected, keys(true)) end
    panel.render(current_review(), snapshot.comments, selected, keys(), openPanel == true, tab)
  end)
end
function M.threads() M.refresh(true) end
local function open_review()
  if tab then diff.detach(tab) end
  opening = true
  diff.open(context, current_review())
end
function M.create(kind, repo, pr)
  request({ op = 'create', kind = kind, repo = repo, pr = pr, actor = config.actor }, function(out)
    selected = out.state.reviews[#out.state.reviews].id
    open_review()
  end)
end
function M.select()
  request({ op = 'state' }, function()
    local choices = { { action = 'local', label = 'Local worktree — new draft' }, { action = 'github', label = 'GitHub — open pull requests' }, { action = 'notes', label = 'Standalone checkout notes' } }
    for _, r in ipairs(snapshot.reviews) do table.insert(choices, { id = r.id, label = 'Continue ' .. r.kind .. ' ' .. r.repo .. ' #' .. r.pr .. ' · ' .. r.status }) end
    vim.ui.select(choices, { prompt = 'LGTM · choose a review', format_item = function(item) return item.label end }, function(item)
      if not item then return end
      if item.action == 'local' then M.create('local')
      elseif item.action == 'github' then
        request({ op = 'pulls' }, function(out)
          if not out.pulls or #out.pulls == 0 then vim.notify('No open pull requests for ' .. (out.repository or 'this checkout')); return end
          vim.ui.select(out.pulls, { prompt = 'Open PRs · ' .. out.repository,
            format_item = function(pr) return '#' .. pr.number .. ' · ' .. pr.title .. (pr.draft and ' · draft' or '') end }, function(pr)
            if pr then M.create('github', out.repository, pr.number) end
          end)
        end)
      elseif item.action == 'notes' then selected = ''; open_review()
      else selected = item.id; open_review() end
    end)
  end)
end
function M.new() M.select() end
function M.diff() if context then open_review() else M.launch() end end
function M.comment(standalone)
  if not context then vim.notify('Launch :lgtm first'); return end
  local anchor = diff.anchor(context.root, tab)
  if not anchor then vim.notify('Place the cursor on a CodeDiff line or a file in this checkout'); return end
  local targetReview = standalone and '' or selected
  compose.open((standalone and 'Standalone note' or 'Comment') .. ' · ' .. anchor.path .. ':' .. anchor.line .. ' · ' .. anchor.side, '', function(body, close, retry)
    request({ op = 'comment', path = anchor.path, line = anchor.line, side = anchor.side, body = body, actor = config.actor, reviewId = targetReview }, function() close(); M.refresh(true) end, retry)
  end)
end
function M.summary()
  local r = current_review(); if not r then vim.notify('Choose a draft review first'); return end
  compose.open('Review summary', r.body, function(body, close, retry) request({ op = 'edit', id = r.id, body = body }, function() close(); M.refresh(true) end, retry) end)
end
local function thread_target(callback)
  local c = panel.target()
  if c then callback(c) else diff.target(callback) end
end
function M.reply()
  thread_target(function(c)
    compose.open('Reply · ' .. c.path .. ':' .. c.line, '', function(body, close, retry) request({ op = 'reply', id = c.id, body = body, actor = config.actor }, function() close(); M.refresh(true) end, retry) end)
  end)
end
function M.resolve()
  thread_target(function(c) request({ op = 'resolve', id = c.id, event = c.resolved and 'REOPEN' or 'RESOLVE' }, function() M.refresh(true) end) end)
end
function M.promote()
  request({ op = 'state' }, function()
    local comments, reviews = {}, {}
    for _, c in ipairs(snapshot.comments) do if not c.threadId and c.reviewId == '' then table.insert(comments, c) end end
    for _, r in ipairs(snapshot.reviews) do if r.status == 'draft' then table.insert(reviews, r) end end
    vim.ui.select(comments, { prompt = 'Standalone note', format_item = function(c) return c.path .. ':' .. c.line .. ' ' .. c.body end }, function(c)
      if not c then return end
      vim.ui.select(reviews, { prompt = 'Promote to review', format_item = function(r) return r.kind .. ' ' .. r.repo .. ' #' .. r.pr .. ' · ' .. r.id end }, function(r)
        if r then request({ op = 'promote', id = c.id, reviewId = r.id }, function() selected = r.id; M.refresh(true) end) end
      end)
    end)
  end)
end
local submitting = false
function M.submit(event)
  if submitting then vim.notify('A review submission is already in progress'); return end
  request({ op = 'state' }, function()
    local r = current_review()
    if not r then vim.notify('Choose a draft review first'); return end
    if r.status ~= 'draft' then vim.notify('This review is already submitted (' .. r.status .. '). Create a new draft.'); return end
    if not event then
      local actions = { { event = 'COMMENT', label = 'Submit review comments' }, { event = 'APPROVE', label = 'Approve' }, { event = 'REQUEST_CHANGES', label = 'Request changes' } }
      vim.ui.select(actions, { prompt = 'LGTM · finish review', format_item = function(action) return action.label end }, function(action) if action then M.submit(action.event) end end)
      return
    end
    compose.open('Finish review · ' .. event .. ' · summary', r.body, function(body, close, retry)
      if event == 'REQUEST_CHANGES' and body:match('^%s*$') then
        vim.notify('Describe the requested changes in the summary', vim.log.levels.WARN); retry(); return
      end
      request({ op = 'edit', id = r.id, body = body }, function()
        local destination = r.kind == 'github' and (r.repo .. ' #' .. r.pr .. ' on GitHub') or 'this local review'
        vim.ui.select({ 'Cancel', 'Submit ' .. event }, { prompt = 'Submit to ' .. destination .. '?' }, function(choice)
          if choice ~= 'Submit ' .. event then retry(); return end
          submitting = true
          request({ op = 'submit', id = r.id, event = event }, function()
            submitting = false; close(); M.refresh(true)
            vim.notify('Review submitted: ' .. event, vim.log.levels.INFO, { title = 'LGTM' })
          end, function() submitting = false; retry() end)
        end)
      end, retry)
    end, { allow_empty = true })
  end)
end
function M.history()
  request({ op = 'history', id = selected }, function(out)
    compose.open('GitHub discussion (read only)', out.text or '', function() end)
    vim.cmd('stopinsert')
    vim.bo.buftype = 'nofile'; vim.bo.modified = false; vim.bo.modifiable = false
  end)
end
function M.identity()
  vim.ui.input({ prompt = 'Comment author: ', default = config.actor.name }, function(name)
    if not name or name == '' then return end
    vim.ui.select({ 'human', 'agent' }, { prompt = 'Author kind' }, function(kind) if kind then config.actor = { name = name, kind = kind }; vim.notify('Commenting as ' .. name .. ' [' .. kind .. ']') end end)
  end)
end
function M.launch()
  local function launch_root(root)
    service.ensure(root, config, function(ready)
      context = ready; selected = ''; snapshot = { reviews = {}, comments = {} }
      M.select()
    end)
  end
  local cwd = config.root or vim.fn.getcwd()
  local result = vim.system({ 'git', '-C', cwd, 'rev-parse', '--show-toplevel' }, { text = true }):wait()
  if result.code ~= 0 then vim.notify('LGTM needs a Git checkout', vim.log.levels.ERROR); return end
  launch_root(vim.uv.fs_realpath(vim.trim(result.stdout)))
end
function M.setup(opts)
  config = vim.tbl_deep_extend('force', config, opts or {})
  vim.api.nvim_create_user_command('Lgtm', function() M.launch() end, { force = true })
  -- Neovim requires uppercase user-command names. Expand the requested :lgtm
  -- spelling only as the first command-line word.
  vim.cmd([[cnoreabbrev <expr> lgtm (getcmdtype() == ':' && getcmdpos() == 5) ? 'Lgtm' : 'lgtm']])
  local commands = { LgtmNew = M.new, LgtmSelect = M.select, LgtmThreads = M.threads, LgtmDiff = M.diff,
    LgtmHistory = M.history, LgtmRefresh = M.refresh, LgtmIdentity = M.identity,
    LgtmComment = function() M.comment(false) end, LgtmNote = function() M.comment(true) end,
    LgtmPromote = M.promote, LgtmSummary = M.summary, LgtmReply = M.reply, LgtmResolve = M.resolve,
    LgtmSubmit = function() M.submit() end, LgtmApprove = function() M.submit('APPROVE') end,
    LgtmReject = function() M.submit('REQUEST_CHANGES') end }
  for name, callback in pairs(commands) do vim.api.nvim_create_user_command(name, callback, { force = true }) end
  local group = vim.api.nvim_create_augroup('LGTMCodeDiff', { clear = true })
  vim.api.nvim_create_autocmd('User', { group = group, pattern = 'CodeDiffOpen', callback = function(event)
    if not opening then return end
    opening = false; tab = event.data and event.data.tabpage or vim.api.nvim_get_current_tabpage()
    diff.bind(tab, keys(true))
    vim.schedule(function() M.refresh(true) end)
  end })
  vim.api.nvim_create_autocmd('User', { group = group, pattern = 'CodeDiffFileSelect', callback = function(event)
    if tab and (not event.data or event.data.tabpage == tab) then vim.schedule(function() M.refresh() end) end
  end })
  local function attach_ready()
    if not tab or vim.api.nvim_get_current_tabpage() ~= tab then return end
    -- FileSelect precedes asynchronous view replacement. Attach again after
    -- buffer/window entry and virtual-file loading finish.
    vim.schedule(function()
      if tab and vim.api.nvim_get_current_tabpage() == tab then
        diff.bind(tab, keys(true))
        M.refresh()
      end
    end)
  end
  vim.api.nvim_create_autocmd({ 'BufEnter', 'BufWinEnter', 'WinEnter', 'TabEnter' }, { group = group, callback = attach_ready })
  vim.api.nvim_create_autocmd('User', { group = group, pattern = 'CodeDiffVirtualFileLoaded', callback = attach_ready })
  vim.api.nvim_create_autocmd('TabLeave', { group = group, callback = function()
    if tab and vim.api.nvim_get_current_tabpage() == tab then diff.detach(tab) end
  end })
  vim.api.nvim_create_autocmd('User', { group = group, pattern = 'CodeDiffClose', callback = function(event)
    local closed = event.data and event.data.tabpage
    if closed then diff.detach(closed) end
    if closed == tab then tab = nil end
  end })
  vim.api.nvim_create_autocmd({ 'VimResized', 'ColorScheme' }, { group = group, callback = function()
    if tab then vim.schedule(function() M.refresh() end) end
  end })
  vim.api.nvim_create_autocmd('VimLeavePre', { group = group, callback = service.stop })
end
return M
