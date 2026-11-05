local AnchoredThreadSummary = require("lgtm.ui.anchored_thread_summary")
local Object = require("nui.object")

local ThreadsOverlay = Object("ThreadsOverlay")

local ns_id = vim.api.nvim_create_namespace("lgtm_threads_overlay")

function ThreadsOverlay:init(opts)
  assert(type(opts) == "table", "ThreadsOverlay options are required")
  self.local_service = assert(opts.local_service, "local_service is required")
  self.pr_service = assert(opts.pr_service, "pr_service is required")
  self.get_mode_for_buf = opts.get_mode_for_buf
  self.diff_buffers = {}
  self.extmark_threads = {}
  self.subscriptions = {}
  self.augroup = nil
end

function ThreadsOverlay.create(opts)
  return ThreadsOverlay(opts)
end

function ThreadsOverlay:attach()
  self:detach()
  local refresh = vim.schedule_wrap(function()
    self:refresh_all()
  end)

  table.insert(self.subscriptions, self.local_service.state.threads:observe(refresh))
  table.insert(self.subscriptions, self.local_service.state.selected_thread_id:observe(refresh))
  table.insert(self.subscriptions, self.pr_service.state.threads:observe(refresh))
  table.insert(self.subscriptions, self.pr_service.state.selected_thread_id:observe(refresh))

  self.augroup = vim.api.nvim_create_augroup("LgtmThreadsOverlay", { clear = true })
  vim.api.nvim_create_autocmd({ "BufEnter", "BufWinEnter", "TextChanged", "TextChangedI" }, {
    group = self.augroup,
    callback = function(ev)
      self:refresh_buffer(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd({ "WinResized", "VimResized" }, {
    group = self.augroup,
    callback = function()
      self:refresh_all()
    end,
  })
  self:refresh_all()
end

function ThreadsOverlay:detach()
  for _, sub in ipairs(self.subscriptions) do
    if sub and sub.unsubscribe then
      sub:unsubscribe()
    end
  end
  self.subscriptions = {}
  if self.augroup then
    pcall(vim.api.nvim_del_augroup_by_id, self.augroup)
    self.augroup = nil
  end
end

function ThreadsOverlay:refresh_buffer(bufnr)
  if not bufnr or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  vim.api.nvim_buf_clear_namespace(bufnr, ns_id, 0, -1)
  self.extmark_threads[bufnr] = {}

  local context = self:_resolve_buffer_context(bufnr)
  if not context then
    return
  end

  local line_count = math.max(1, vim.api.nvim_buf_line_count(bufnr))
  local width = self:_buffer_width(bufnr)
  for _, thread in ipairs(context.threads) do
    self:_place_thread_extmark(bufnr, thread, context.selected_id, line_count, width)
  end
end

function ThreadsOverlay:refresh_all()
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) then
      self:refresh_buffer(bufnr)
    end
  end
end

function ThreadsOverlay:register_diff_buffer(bufnr, path, side)
  if not bufnr or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  self.diff_buffers[bufnr] = {
    path = self:_normalize_path(path),
    side = side or "RIGHT",
  }
  self:refresh_buffer(bufnr)
end

function ThreadsOverlay:get_buffer_target(bufnr)
  local registered = self.diff_buffers[bufnr]
  if registered and registered.path ~= "" then
    return registered.path, registered.side
  end
  return self:_buffer_relative_path(bufnr), "RIGHT"
end

function ThreadsOverlay:get_thread_at_cursor(winid)
  winid = winid or vim.api.nvim_get_current_win()
  if not vim.api.nvim_win_is_valid(winid) then
    return nil
  end
  local bufnr = vim.api.nvim_win_get_buf(winid)
  local line = vim.api.nvim_win_get_cursor(winid)[1]
  local by_line = self.extmark_threads[bufnr] and self.extmark_threads[bufnr][line] or nil
  if by_line then
    return by_line
  end
  local context = self:_resolve_buffer_context(bufnr)
  if not context then
    return nil
  end
  return context.service:get_thread_at_line(context.path, line, context.side)
end

function ThreadsOverlay:jump_thread(direction, winid)
  winid = winid or vim.api.nvim_get_current_win()
  local bufnr = vim.api.nvim_win_get_buf(winid)
  local context = self:_resolve_buffer_context(bufnr)
  if not context or #context.threads == 0 then
    return nil
  end
  local current_line = vim.api.nvim_win_get_cursor(winid)[1]
  local target = self:_pick_adjacent_thread(context.threads, current_line, direction)
  if not target then
    return nil
  end
  local max_line = math.max(1, vim.api.nvim_buf_line_count(bufnr))
  local dest_line = math.max(1, math.min(target.line, max_line))
  vim.api.nvim_win_set_cursor(winid, { dest_line, 0 })
  context.service:select_thread(target.id)
  return target
end

function ThreadsOverlay.get_namespace()
  return ns_id
end

function ThreadsOverlay:_place_thread_extmark(bufnr, thread, selected_id, line_count, width)
  local target_row = math.max(0, math.min(thread.line - 1, line_count - 1))
  local is_selected = thread.id == selected_id
  local card = AnchoredThreadSummary(thread, {
    is_selected = is_selected,
    width = width,
  })
  local extmark_id = vim.api.nvim_buf_set_extmark(bufnr, ns_id, target_row, 0, {
    virt_lines = card:render(),
    virt_lines_above = false,
    virt_lines_leftcol = true,
    number_hl_group = "CursorLineNr",
    sign_text = "┃",
    sign_hl_group = card.sign_hl,
    priority = 1100,
    hl_eol = true,
  })
  self.extmark_threads[bufnr][target_row + 1] = thread
  self.extmark_threads[bufnr]["extmark:" .. tostring(extmark_id)] = thread
end

function ThreadsOverlay:_buffer_width(bufnr)
  local wins = vim.fn.win_findbuf(bufnr)
  if wins and wins[1] and vim.api.nvim_win_is_valid(wins[1]) then
    return vim.api.nvim_win_get_width(wins[1])
  end
  return vim.o.columns
end

function ThreadsOverlay:_resolve_buffer_context(bufnr)
  local buftype = vim.bo[bufnr].buftype
  local registered = self.diff_buffers[bufnr]
  if not registered and buftype ~= "" and buftype ~= "acwrite" then
    return nil
  end
  local path, side = self:get_buffer_target(bufnr)
  if path == "" then
    return nil
  end
  local mode = self.get_mode_for_buf and self.get_mode_for_buf(bufnr)
    or (registered and "pr" or "local")
  local service = mode == "pr" and self.pr_service or self.local_service
  local threads = mode == "pr" and service:get_threads_for_file(path, side)
    or service:get_threads_for_file(path)
  local selected_id = service.state:get_value().selected_thread_id
  return {
    mode = mode,
    service = service,
    path = path,
    side = side,
    threads = threads,
    selected_id = selected_id,
  }
end

function ThreadsOverlay:_pick_adjacent_thread(threads, current_line, direction)
  if direction >= 0 then
    for _, thread in ipairs(threads) do
      if thread.line > current_line then
        return thread
      end
    end
    return threads[1]
  end
  for idx = #threads, 1, -1 do
    if threads[idx].line < current_line then
      return threads[idx]
    end
  end
  return threads[#threads]
end

function ThreadsOverlay:_normalize_path(path)
  if type(path) ~= "string" then
    return ""
  end
  local trimmed = (path:gsub("^%s+", ""):gsub("%s+$", "")):gsub("^%./", "")
  return trimmed
end

function ThreadsOverlay:_buffer_relative_path(bufnr)
  local name = vim.api.nvim_buf_get_name(bufnr)
  if name == "" then
    return ""
  end
  local codediff_path = name:match("^codediff://[^/]+/[^/]+/(.+)$")
  if codediff_path then
    return self:_normalize_path(codediff_path)
  end
  local rel = vim.fn.fnamemodify(name, ":.")
  return self:_normalize_path(rel)
end

return ThreadsOverlay
