local function main()
-- Uses the user's ordinary configuration and its actual CodeDiff dependency.
vim.o.columns = 180; vim.o.lines = 60
local root = vim.fn.getcwd()
local fixture = root .. '/.tmp/go'
local temporary = vim.fn.tempname()
assert(vim.fn.exists(':Lgtm') == 2, 'LGTM is not registered in the normal Neovim config')
assert(vim.fn.maparg('lgtm', 'c', true) ~= '', 'lowercase :lgtm alias missing')
vim.v.errmsg = ''
local errors = {}
local notify = vim.notify
vim.notify = function(message, level, opts)
  if level == vim.log.levels.ERROR then table.insert(errors, tostring(message)) end
  notify(message, level, opts)
end
require('lazy').load({ plugins = { 'atlas.nvim' } })
local lgtm = require('lgtm')
vim.cmd('cd ' .. vim.fn.fnameescape(fixture))
lgtm.setup({ actor = { name = 'Claude', kind = 'agent' } })
-- Isolate review state without altering the fixture checkout or its source.
-- A prestarted service supplies a configurable endpoint, then a second launch
-- below exercises automatic service startup on ephemeral ports.
local output = ''
local server = vim.system({ root .. '/bin/lgtm', 'serve', '--repo', fixture, '--db', temporary, '--ui', root .. '/apps/lgtm-web-app/build', '--http', '127.0.0.1:0', '--grpc', '127.0.0.1:0' }, { text = true, stdout = function(_, data) if data then output = output .. data end end })
local function wait(predicate, message) assert(vim.wait(10000, predicate, 40), message) end
local endpoint
local function state()
  local res = vim.system({ 'curl', '--silent', '--max-time', '2', '-H', 'Content-Type: application/json', '--data-binary', '{"op":"state"}', endpoint }, { text = true }):wait()
  local ok, decoded = pcall(vim.json.decode, res.stdout)
  return ok and decoded.state or nil
end
local function save_composer(body)
  wait(function() return vim.api.nvim_buf_get_name(0):find('lgtm-compose://',1,true)~=nil end,'composer did not open')
  local buffer=vim.api.nvim_get_current_buf();vim.cmd('stopinsert');vim.api.nvim_buf_set_lines(0,0,-1,false,vim.split(body,'\n',{plain=true}));vim.cmd('write');wait(function() return not vim.api.nvim_buf_is_valid(buffer) end,'composer did not close after save')
end
local ok, err = pcall(function()
  wait(function() return output:find('web http://',1,true)~=nil end,'test service did not start')
  endpoint = output:match('web (http://[%d%.]+:%d+)') .. '/api'
  lgtm.setup({ endpoint = endpoint })
  vim.ui.select = function(items, opts, callback)
    assert(opts.prompt ~= 'LGTM · launch UI', 'Neovim still shows a web UI chooser')
    callback(items[1])
  end
  -- Expand the lowercase command through the real command line, as typed.
  vim.api.nvim_feedkeys(vim.api.nvim_replace_termcodes(':lgtm<CR>',true,false,true),'xt',false)
  wait(function() local s=state();return s and #s.reviews==1 end,'draft not created through :lgtm')
  local namespace=vim.api.nvim_create_namespace('LGTMComments')
  local function cards(buf)
    local result={}
    for _,mark in ipairs(vim.api.nvim_buf_get_extmarks(buf,namespace,0,-1,{details=true})) do
      if mark[4].virt_lines then for _,row in ipairs(mark[4].virt_lines) do for _,chunk in ipairs(row) do result[#result+1]=chunk[1] end end end
    end
    return table.concat(result,'\n')
  end
  local lifecycle=require('codediff.ui.lifecycle')
  wait(function() return lifecycle.get_session(vim.api.nvim_get_current_tabpage())~=nil end,'CodeDiff did not open')
  local session
  wait(function() session=lifecycle.get_session(vim.api.nvim_get_current_tabpage());return session and session.modified and session.modified.relative~='' end,'CodeDiff did not select a file')
  vim.api.nvim_set_current_win(session.modified_win)
  local line=math.min(2,vim.api.nvim_buf_line_count(session.modified_bufnr));vim.api.nvim_win_set_cursor(0,{line,0})
  vim.api.nvim_feedkeys('c','xt',false);save_composer('Neovim right-side review comment\nWith a second line.')
  wait(function() local s=state();return s and #s.comments==1 end,'right comment not saved')
  assert(state().comments[1].side=='RIGHT');assert(state().comments[1].line==line)
  wait(function()return cards(session.modified_bufnr):find('With a second line.',1,true)~=nil end,'multiline inline card missing')
  assert(cards(session.modified_bufnr):find('Claude [agent]',1,true),'agent identity missing from inline card')
  assert(not cards(session.original_bufnr):find('Neovim right-side',1,true),'right comment appeared on left side')
  lgtm.reply();save_composer('Reply from the source anchor\nSecond reply line')
  wait(function()local s=state();return s and #s.comments==2 end,'reply from source anchor failed')
  wait(function()return cards(session.modified_bufnr):find('Second reply line',1,true)~=nil end,'reply not rendered inline')
  vim.api.nvim_set_current_win(session.original_win);vim.api.nvim_win_set_cursor(0,{line,0});vim.api.nvim_feedkeys('c','xt',false);save_composer('Neovim left-side review comment')
  wait(function() local s=state();return s and #s.comments==3 end,'left comment not saved');assert(state().comments[3].side=='LEFT')
  -- Switch files through the actual explorer, including its one-sided view.
  local owner = vim.api.nvim_get_current_tabpage()
  local explorer = lifecycle.get_panel_view(owner)
  local entries = require('codediff.ui.explorer.tree').get_all_files(explorer.tree)
  local new_file
  for _, node in ipairs(entries) do if node.data.path == 'lgtm-notes.md' then new_file = node.data end end
  assert(new_file, 'untracked file missing from explorer')
  explorer.on_file_select(new_file)
  wait(function()
    session = lifecycle.get_session(owner)
    return session and session.modified.relative == 'lgtm-notes.md' and session.single_side == 'modified'
  end, 'one-sided added file did not open')
  vim.api.nvim_set_current_win(session.modified_win)
  wait(function()return vim.fn.maparg('c','n',false,true).desc == 'LGTM review: c' end,'c mapping missing after async file switch')
  vim.api.nvim_feedkeys('c','xt',false)
  wait(function()return vim.api.nvim_buf_get_name(0):find('lgtm-compose://',1,true)~=nil end,'c did not open editor for one-sided file')
  assert(vim.bo.modifiable and vim.bo.buftype == 'acwrite', 'comment composer is not writable')
  vim.cmd('stopinsert');vim.api.nvim_feedkeys('q','xt',false)
  vim.api.nvim_set_current_win(session.modified_win)
  vim.api.nvim_feedkeys('n','xt',false);save_composer('Neovim standalone note')
  -- Source buffers can be shared by unrelated CodeDiff/Atlas tabs. Hand c
  -- back on tab leave, then restore LGTM ownership on reentry.
  vim.cmd('tabnew')
  local unrelated = vim.api.nvim_get_current_tabpage()
  vim.api.nvim_win_set_buf(0, session.modified_bufnr)
  local foreign_called = false
  local foreign = function() foreign_called = true end
  vim.keymap.set('n','c',foreign,{buffer=session.modified_bufnr,desc='Other review plugin'})
  vim.api.nvim_feedkeys('c','xt',false);assert(foreign_called,'LGTM intercepted another tab’s c mapping')
  vim.api.nvim_set_current_tabpage(owner)
  wait(function()return vim.fn.maparg('c','n',false,true).desc == 'LGTM review: c' end,'LGTM c was not restored on tab return')
  vim.api.nvim_set_current_tabpage(unrelated)
  wait(function()return vim.fn.maparg('c','n',false,true).callback == foreign end,'foreign mapping was not restored')
  vim.cmd('tabclose')
  vim.api.nvim_set_current_tabpage(owner)
  wait(function()return vim.fn.maparg('c','n',false,true).desc == 'LGTM review: c' end,'c mapping missing after foreign tab closed')
  wait(function()local s=state();return s and #s.comments==4 end,'note not saved')
  lgtm.promote();wait(function()local s=state();return s and s.comments[4].reviewId==s.reviews[1].id end,'promotion failed')
  lgtm.summary();save_composer('Neovim review summary');wait(function()local s=state();return s and s.reviews[1].body=='Neovim review summary' end,'summary not saved')
  vim.ui.select=function(items,_,callback) callback(items[2]) end
  lgtm.submit('APPROVE');save_composer('Neovim review summary');wait(function()local s=state();return s and s.reviews[1].status=='APPROVE' end,'approval failed')
  local tab=vim.api.nvim_get_current_tabpage();require('codediff.ui.lifecycle').close(tab)
  if vim.api.nvim_buf_is_valid(session.modified_bufnr) then
    local mapping = vim.api.nvim_buf_call(session.modified_bufnr, function() return vim.fn.maparg('c', 'n', false, true) end)
    assert(mapping.desc ~= 'LGTM review: c', 'LGTM comment mapping remained after CodeDiff close')
    assert(mapping.callback == foreign, 'closing LGTM did not restore the other plugin mapping')
  end
  -- An API failure must leave the composer and draft intact and allow retry.
  vim.ui.select = function(items, opts, callback)
    if opts.prompt:find('Submit to', 1, true) then callback(items[2]) else callback(items[1]) end
  end
  lgtm.create('local');wait(function()local s=state();return s and #s.reviews==2 end,'second draft failed')
  wait(function()local session=lifecycle.get_session(vim.api.nvim_get_current_tabpage());return session and vim.api.nvim_win_is_valid(session.modified_win) end,'second CodeDiff session did not open')
  local transport = require('lgtm.service'); local original_request = transport.request
  local failed = false
  transport.request = function(context, payload, callback, onerror)
    if payload.op == 'submit' and not failed then failed = true; onerror('Simulated remote rejection'); return end
    original_request(context, payload, callback, onerror)
  end
  lgtm.submit()
  wait(function()return vim.api.nvim_buf_get_name(0):find('lgtm-compose://',1,true)~=nil end,'submission summary did not open')
  local composer = vim.api.nvim_get_current_buf()
  vim.cmd('stopinsert');vim.api.nvim_buf_set_lines(composer,0,-1,false,{'Submission retry summary'});vim.cmd('write')
  wait(function()return failed end,'failure not injected')
  assert(vim.api.nvim_buf_is_valid(composer),'submission error closed composer')
  assert(state().reviews[2].status=='draft','submission error discarded draft')
  vim.api.nvim_buf_call(composer, function() vim.cmd('write') end);wait(function()local s=state();return s and s.reviews[2].status=='COMMENT' end,'submission retry failed')
  wait(function()return not vim.api.nvim_buf_is_valid(composer) end,'submission retry did not close composer')
  transport.request = original_request
  lifecycle.close(vim.api.nvim_get_current_tabpage())
  local launched
  require('lgtm.service').ensure(fixture,{project=root},function(context)launched=context end)
  wait(function()return launched and launched.endpoint end,'automatic service launch failed')
  assert(launched.url:match('http://127.0.0.1:'))
  assert(vim.v.errmsg == '', vim.v.errmsg)
  assert(#errors == 0, table.concat(errors, '\n'))
  print('Normal Neovim: lowercase launcher, actual c/n keypresses with Atlas loaded, one-sided file switches, foreign mapping restoration, left/right anchors, inline multiline cards and replies, notes, promotion, submission retry, approval, and automatic service startup passed')
end)
require('lgtm.service').stop();server:kill(2);server:wait(5000);vim.fn.delete(temporary)
if not ok then io.stderr:write(tostring(err)..'\n');vim.cmd('cquit 1') end
vim.cmd('qa!')

end
local ok, err = pcall(main)
if not ok then io.stderr:write(tostring(err) .. "\n"); vim.cmd("cquit 1") end
