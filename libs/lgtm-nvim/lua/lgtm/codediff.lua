local M = {}
local attached = {}
local targets = {}
local inline = require("lgtm.inline")
local namespace = vim.api.nvim_create_namespace('LGTMComments')
local function lifecycle() return require('codediff.ui.lifecycle') end
function M.open(context, review)
  if review and review.kind == 'github' then
    require('codediff.commands.handlers.pull_request').run(review.pr, { remote = 'origin' }, { repo = context.root, layout = 'side-by-side' })
  else
    require('codediff.commands.handlers.explorer').run('HEAD', nil, { repo = context.root, layout = 'side-by-side' })
  end
end
function M.anchor(root, active_tab)
  local tab = vim.api.nvim_get_current_tabpage()
  local session = lifecycle().get_session(tab)
  local buf = vim.api.nvim_get_current_buf()
  if session and active_tab and tab ~= active_tab then return end
  if session and session.git_root == root then
    local side, ref
    if buf == session.original_bufnr then side = 'LEFT'; ref = session.original
    elseif buf == session.modified_bufnr then side = 'RIGHT'; ref = session.modified end
    if ref and ref.relative ~= '' then return { path = ref.relative, side = side, line = vim.api.nvim_win_get_cursor(0)[1] } end
  end
  local file = vim.api.nvim_buf_get_name(buf)
  if vim.bo[buf].buftype == '' and file:sub(1, #root + 1) == root .. '/' then
    return { path = file:sub(#root + 2), side = 'RIGHT', line = vim.api.nvim_win_get_cursor(0)[1] }
  end
end
function M.bind(tab, callbacks)
  local session = lifecycle().get_session(tab)
  if not session or vim.api.nvim_get_current_tabpage() ~= tab then return end
  local keep = {}
  for _, side in ipairs({'original', 'modified'}) do
    local buf = session[side .. '_bufnr']
    if buf and vim.api.nvim_buf_is_valid(buf) then keep[buf] = true end
  end
  M.detach(tab, keep)
  attached[tab] = attached[tab] or {}
  for buf in pairs(keep) do
    attached[tab][buf] = attached[tab][buf] or {}
    for key, callback in pairs(callbacks) do
      local record = attached[tab][buf][key]
      if not record then
        local old = vim.api.nvim_buf_call(buf, function() return vim.fn.maparg(key, 'n', false, true) end)
        record = { previous = old.buffer == 1 and old or false, callback = callback }
        attached[tab][buf][key] = record
      end
      record.callback = callback
      vim.keymap.set('n', key, callback, { buffer = buf, silent = true, desc = 'LGTM review: ' .. key })
    end
  end
end
function M.mark(tab, comments, reviewId, callbacks)
  if vim.api.nvim_get_current_tabpage() ~= tab then return end
  local session = lifecycle().get_session(tab)
  if not session then return end
  M.bind(tab, callbacks)
  inline.setup()
  for _, buf in ipairs({session.original_bufnr, session.modified_bufnr}) do
    if vim.api.nvim_buf_is_valid(buf) then vim.api.nvim_buf_clear_namespace(buf, namespace, 0, -1); targets[buf] = {} end
  end
  for _, side in ipairs({ 'LEFT', 'RIGHT' }) do
    local buf = side == 'LEFT' and session.original_bufnr or session.modified_bufnr
    local ref = side == 'LEFT' and session.original or session.modified
    local win = side == 'LEFT' and session.original_win or session.modified_win
    if buf and vim.api.nvim_buf_is_valid(buf) and win and vim.api.nvim_win_is_valid(win) and vim.api.nvim_win_get_buf(win) == buf then
      for _, c in ipairs(comments) do
        if not c.threadId and not c.resolved and ref and ref.relative == c.path and c.side == side and (c.reviewId == reviewId or c.reviewId == '') then
          local row = math.min(c.line - 1, vim.api.nvim_buf_line_count(buf) - 1)
          if row >= 0 then
            local width = vim.api.nvim_win_get_width(win)
            local lines = inline.lines(c, comments, width)
            vim.api.nvim_buf_set_extmark(buf, namespace, row, 0, { sign_text = '┃', sign_hl_group = 'LgtmHuman',
              virt_lines = lines, virt_lines_leftcol = true, priority = 1100 })
            targets[buf][row + 1] = targets[buf][row + 1] or {}
            table.insert(targets[buf][row + 1], c)
            -- Keep both CodeDiff panes aligned after inserting the comment card.
            local other = side == 'LEFT' and session.modified_bufnr or session.original_bufnr
            local line, offset = row + 1, 0
            for _, change in ipairs((session.stored_diff_result or {}).changes or {}) do
              local source = side == 'LEFT' and change.original or change.modified
              local dest = side == 'LEFT' and change.modified or change.original
              if line < source.start_line then break end
              if line < source.end_line then
                line = dest.start_line + math.min(line - source.start_line, math.max(0, dest.end_line - dest.start_line - 1)); offset = 0; break
              end
              offset = dest.end_line - source.end_line
            end
            if other and other ~= buf and vim.api.nvim_buf_is_valid(other) then
            local opposite = math.max(0, math.min(line + offset - 1, vim.api.nvim_buf_line_count(other) - 1))
            local padding = {}; for _ = 1, #lines do table.insert(padding, {{'', 'Normal'}}) end
            vim.api.nvim_buf_set_extmark(other, namespace, opposite, 0, { virt_lines = padding, virt_lines_leftcol = true, priority = 1090 })
            end
          end
        end
      end

    end
  end
end
function M.target(callback)
  local items = (targets[vim.api.nvim_get_current_buf()] or {})[vim.api.nvim_win_get_cursor(0)[1]] or {}
  if #items == 1 then callback(items[1])
  elseif #items > 1 then vim.ui.select(items, { prompt = 'Thread at this line', format_item = function(c) return c.actor.name .. ': ' .. c.body:gsub('\n', ' ') end }, callback)
  else vim.notify('Place the cursor on a commented line or select a thread in :LgtmThreads') end
end
function M.detach(tab, keep)
  for buf, mappings in pairs(attached[tab] or {}) do
    if not keep or not keep[buf] then
      if vim.api.nvim_buf_is_valid(buf) then
        vim.api.nvim_buf_clear_namespace(buf, namespace, 0, -1)
        for key, record in pairs(mappings) do
          local current = vim.api.nvim_buf_call(buf, function() return vim.fn.maparg(key, 'n', false, true) end)
          if current.callback == record.callback then
          local old = record.previous
          pcall(vim.keymap.del, 'n', key, { buffer = buf })
          if old then
            vim.keymap.set('n', key, old.callback or old.rhs, { buffer = buf, silent = old.silent == 1,
              expr = old.expr == 1, remap = old.noremap == 0, desc = old.desc })
          end
          end
        end
      end
      targets[buf] = nil
      attached[tab][buf] = nil
    end
  end
  if not keep then attached[tab] = nil end
end
return M
