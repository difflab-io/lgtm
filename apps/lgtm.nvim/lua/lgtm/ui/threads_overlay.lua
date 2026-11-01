local AnchoredThreadSummary = require("lgtm.ui.anchored_thread_summary")

local ThreadsOverlay = {}
ThreadsOverlay.__index = ThreadsOverlay

local UI = {}
local Private = {}

-- State -----------------------------------------------------------------------

local ns_id = vim.api.nvim_create_namespace("lgtm_threads_overlay")

---Construct a ThreadsOverlay manager bound to LocalReviewService and PrService.
---@param opts { local_service: table, pr_service: table, get_mode_for_buf?: fun(bufnr: integer): string }
---@return table
function ThreadsOverlay.new(opts)
  assert(type(opts) == "table", "ThreadsOverlay options are required")
  local self = setmetatable({
    local_service = assert(opts.local_service, "local_service is required"),
    pr_service = assert(opts.pr_service, "pr_service is required"),
    get_mode_for_buf = opts.get_mode_for_buf,
    diff_buffers = {},
    extmark_threads = {},
    subscriptions = {},
    augroup = nil,
  }, ThreadsOverlay)
  return self
end

-- Render ----------------------------------------------------------------------

---Re-render thread extmarks inside a single buffer.
---@param bufnr integer
function ThreadsOverlay:refresh_buffer(bufnr)
  if not bufnr or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  vim.api.nvim_buf_clear_namespace(bufnr, ns_id, 0, -1)
  self.extmark_threads[bufnr] = {}

  local context = Private.resolve_buffer_context(self, bufnr)
  if not context then
    return
  end

  local line_count = math.max(1, vim.api.nvim_buf_line_count(bufnr))
  for _, thread in ipairs(context.threads) do
    UI.place_thread_extmark(self, bufnr, thread, context.selected_id, line_count)
  end
end

---Re-render thread extmarks across all valid loaded buffers.
function ThreadsOverlay:refresh_all()
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) then
      self:refresh_buffer(bufnr)
    end
  end
end

-- UI --------------------------------------------------------------------------

function UI.place_thread_extmark(overlay, bufnr, thread, selected_id, line_count)
  local target_row = math.max(0, math.min(thread.line - 1, line_count - 1))
  local is_selected = thread.id == selected_id
  local virt_lines = AnchoredThreadSummary.render_virt_lines(thread, {
    is_selected = is_selected,
  })
  local sign_hl = thread.resolved and "DiagnosticOk" or "DiagnosticWarn"
  local extmark_id = vim.api.nvim_buf_set_extmark(bufnr, ns_id, target_row, 0, {
    virt_lines = virt_lines,
    virt_lines_above = false,
    sign_text = "┃",
    sign_hl_group = sign_hl,
    hl_eol = true,
  })
  overlay.extmark_threads[bufnr][target_row + 1] = thread
  overlay.extmark_threads[bufnr]["extmark:" .. tostring(extmark_id)] = thread
end

-- Lifecycle -------------------------------------------------------------------

---Subscribe to service signals and buffer autocommands.
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
  self:refresh_all()
end

---Unsubscribe from signals and clear buffer autocommands.
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

-- Methods ---------------------------------------------------------------------

---Register a `CodeDiff` buffer with its repository-relative file path and diff side.
---@param bufnr integer
---@param path string
---@param side "LEFT"|"RIGHT"|nil
function ThreadsOverlay:register_diff_buffer(bufnr, path, side)
  if not bufnr or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  self.diff_buffers[bufnr] = {
    path = Private.normalize_path(path),
    side = side or "RIGHT",
  }
  self:refresh_buffer(bufnr)
end

---Return the repository-relative path and diff side associated with `bufnr`.
---@param bufnr integer
---@return string, "LEFT"|"RIGHT"
function ThreadsOverlay:get_buffer_target(bufnr)
  local registered = self.diff_buffers[bufnr]
  if registered and registered.path ~= "" then
    return registered.path, registered.side
  end
  return Private.buffer_relative_path(bufnr), "RIGHT"
end

---Return the anchored thread at or covering the cursor line in `winid`.
---@param winid integer|nil
---@return LgtmThread|nil
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
  local context = Private.resolve_buffer_context(self, bufnr)
  if not context then
    return nil
  end
  return context.service:get_thread_at_line(context.path, line, context.side)
end

---Jump the cursor to the next (`1`) or previous (`-1`) anchored thread in `winid`.
---@param direction 1|-1
---@param winid integer|nil
---@return LgtmThread|nil
function ThreadsOverlay:jump_thread(direction, winid)
  winid = winid or vim.api.nvim_get_current_win()
  local bufnr = vim.api.nvim_win_get_buf(winid)
  local context = Private.resolve_buffer_context(self, bufnr)
  if not context or #context.threads == 0 then
    return nil
  end
  local current_line = vim.api.nvim_win_get_cursor(winid)[1]
  local target = Private.pick_adjacent_thread(context.threads, current_line, direction)
  if not target then
    return nil
  end
  local max_line = math.max(1, vim.api.nvim_buf_line_count(bufnr))
  local dest_line = math.max(1, math.min(target.line, max_line))
  vim.api.nvim_win_set_cursor(winid, { dest_line, 0 })
  context.service:select_thread(target.id)
  return target
end

---Return the namespace ID used by the overlay extmarks.
---@return integer
function ThreadsOverlay.get_namespace()
  return ns_id
end

-- Private ---------------------------------------------------------------------

function Private.resolve_buffer_context(overlay, bufnr)
  local buftype = vim.bo[bufnr].buftype
  local registered = overlay.diff_buffers[bufnr]
  if not registered and buftype ~= "" and buftype ~= "acwrite" then
    return nil
  end
  local path, side = overlay:get_buffer_target(bufnr)
  if path == "" then
    return nil
  end
  local mode = overlay.get_mode_for_buf and overlay.get_mode_for_buf(bufnr)
    or (registered and "pr" or "local")
  local service = mode == "pr" and overlay.pr_service or overlay.local_service
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

function Private.pick_adjacent_thread(threads, current_line, direction)
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

function Private.normalize_path(path)
  if type(path) ~= "string" then
    return ""
  end
  local trimmed = (path:gsub("^%s+", ""):gsub("%s+$", "")):gsub("^%./", "")
  return trimmed
end

function Private.buffer_relative_path(bufnr)
  local name = vim.api.nvim_buf_get_name(bufnr)
  if name == "" then
    return ""
  end
  local codediff_path = name:match("^codediff://[^/]+/[^/]+/(.+)$")
  if codediff_path then
    return Private.normalize_path(codediff_path)
  end
  local rel = vim.fn.fnamemodify(name, ":.")
  return Private.normalize_path(rel)
end

return ThreadsOverlay
