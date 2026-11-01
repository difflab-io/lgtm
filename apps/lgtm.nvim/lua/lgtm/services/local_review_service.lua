local n = require("nui-components")
local Thread = require("lgtm.models.thread")

local LocalReviewService = {}
LocalReviewService.__index = LocalReviewService

local Private = {}

-- State -----------------------------------------------------------------------

---Construct a reactive LocalReviewService managing always-on local review threads.
---@param opts table|nil
---@return table
function LocalReviewService.new(opts)
  opts = opts or {}
  local initial_threads = {}
  for _, raw in ipairs(opts.initial_threads or {}) do
    local payload = vim.tbl_extend("force", {}, raw, { mode = "local" })
    table.insert(initial_threads, Thread.new(payload))
  end

  local self = setmetatable({
    state = n.create_signal({
      threads = initial_threads,
      selected_thread_id = initial_threads[1] and initial_threads[1].id or nil,
      active_view = "summary",
      error = nil,
    }),
  }, LocalReviewService)

  return self
end

-- Methods ---------------------------------------------------------------------

---Reset all local review threads and view state.
function LocalReviewService:reset()
  self.state.threads = {}
  self.state.selected_thread_id = nil
  self.state.active_view = "summary"
  self.state.error = nil
end

---Return all local review threads, optionally filtered by path or resolved status.
---@param opts { path?: string, include_resolved?: boolean }|nil
---@return LgtmThread[]
function LocalReviewService:get_threads(opts)
  opts = opts or {}
  local snapshot = self.state:get_value()
  local normalized_path = opts.path and Private.normalize_path(opts.path) or nil
  local include_resolved = opts.include_resolved ~= false
  local list = {}
  for _, thread in ipairs(snapshot.threads) do
    local path_matches = not normalized_path or thread.path == normalized_path
    local resolved_matches = include_resolved or not thread.resolved
    if path_matches and resolved_matches then
      table.insert(list, thread)
    end
  end
  return list
end

---Return all local review threads anchored to the given file path, sorted by line.
---@param path string
---@return LgtmThread[]
function LocalReviewService:get_threads_for_file(path)
  local list = self:get_threads({ path = path, include_resolved = true })
  table.sort(list, function(a, b)
    if a.line == b.line then
      return a.id < b.id
    end
    return a.line < b.line
  end)
  return list
end

---Return a single local thread by ID, or nil if not found.
---@param thread_id string|nil
---@return LgtmThread|nil
function LocalReviewService:get_thread(thread_id)
  if not thread_id then
    return nil
  end
  local snapshot = self.state:get_value()
  for _, thread in ipairs(snapshot.threads) do
    if thread.id == thread_id then
      return thread
    end
  end
  return nil
end

---Return the first thread anchored at or covering `line` in `path`.
---@param path string
---@param line integer
---@return LgtmThread|nil
function LocalReviewService:get_thread_at_line(path, line)
  local threads = self:get_threads_for_file(path)
  for _, thread in ipairs(threads) do
    if line >= thread.start_line and line <= thread.line then
      return thread
    end
  end
  return nil
end

---Create a new local review thread and select it.
---@param input { path: string, start_line?: integer, line: integer, body: string, author?: string }
---@return LgtmThread
function LocalReviewService:create_thread(input)
  local payload = vim.tbl_extend("force", {}, input or {}, {
    mode = "local",
    side = "RIGHT",
  })
  local created = Thread.new(payload)
  local snapshot = self.state:get_value()
  local next_threads = Private.clone_list(snapshot.threads)
  table.insert(next_threads, created)
  Private.sort_threads(next_threads)
  self.state.threads = next_threads
  self.state.selected_thread_id = created.id
  self.state.error = nil
  return created
end

---Append a reply comment to an existing local thread.
---@param thread_id string
---@param body string
---@param author string|nil
---@return LgtmThread, LgtmComment
function LocalReviewService:reply_to_thread(thread_id, body, author)
  local updated_thread = nil
  local created_comment = nil
  self.state.threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread, created_comment = Thread.with_reply(thread, {
        body = body,
        author = author,
        state = "DRAFT",
      })
      return updated_thread
    end
  )
  self.state.selected_thread_id = thread_id
  return updated_thread, created_comment
end

---Edit an existing comment in a local thread.
---@param thread_id string
---@param comment_id string
---@param body string
---@return LgtmThread, LgtmComment
function LocalReviewService:edit_comment(thread_id, comment_id, body)
  local updated_thread = nil
  local updated_comment = nil
  self.state.threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread, updated_comment = Thread.with_edited_comment(thread, comment_id, body)
      return updated_thread
    end
  )
  return updated_thread, updated_comment
end

---Delete a comment from a local thread, removing the thread if no comments remain.
---@param thread_id string
---@param comment_id string
---@return LgtmThread|nil
function LocalReviewService:delete_comment(thread_id, comment_id)
  local snapshot = self.state:get_value()
  local next_threads = {}
  local found = false
  local resulting_thread = nil

  for _, thread in ipairs(snapshot.threads) do
    if thread.id == thread_id then
      found = true
      resulting_thread = Thread.without_comment(thread, comment_id)
      if resulting_thread then
        table.insert(next_threads, resulting_thread)
      end
    else
      table.insert(next_threads, thread)
    end
  end

  assert(found, "thread not found: " .. tostring(thread_id))
  self.state.threads = next_threads
  if not resulting_thread and snapshot.selected_thread_id == thread_id then
    self.state.selected_thread_id = next_threads[1] and next_threads[1].id or nil
    self.state.active_view = "summary"
  end
  return resulting_thread
end

---Toggle the resolved state of a local thread.
---@param thread_id string
---@return LgtmThread
function LocalReviewService:toggle_resolved(thread_id)
  local updated_thread = nil
  self.state.threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread = Thread.with_resolved(thread, not thread.resolved)
      return updated_thread
    end
  )
  return updated_thread
end

---Toggle the collapsed state of a local thread in summary/overlay views.
---@param thread_id string
---@return LgtmThread
function LocalReviewService:toggle_collapsed(thread_id)
  local updated_thread = nil
  self.state.threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread = Thread.with_collapsed(thread, not thread.collapsed)
      return updated_thread
    end
  )
  return updated_thread
end

---Collapse or uncollapse all local review threads.
---@param collapsed boolean
function LocalReviewService:set_all_collapsed(collapsed)
  local snapshot = self.state:get_value()
  local next_threads = {}
  for _, thread in ipairs(snapshot.threads) do
    table.insert(next_threads, Thread.with_collapsed(thread, collapsed))
  end
  self.state.threads = next_threads
end

---Set the currently highlighted/selected thread ID.
---@param thread_id string|nil
function LocalReviewService:select_thread(thread_id)
  if thread_id ~= nil then
    assert(self:get_thread(thread_id) ~= nil, "thread not found: " .. tostring(thread_id))
  end
  self.state.selected_thread_id = thread_id
end

---Select a thread and transition the threads pane state into `detail` view.
---@param thread_id string
---@return LgtmThread
function LocalReviewService:open_thread_detail(thread_id)
  local thread = assert(self:get_thread(thread_id), "thread not found: " .. tostring(thread_id))
  self.state.selected_thread_id = thread.id
  self.state.active_view = "detail"
  return thread
end

---Return the threads pane state from `detail` view back to `summary` view.
function LocalReviewService:close_thread_detail()
  self.state.active_view = "summary"
end

-- Private ---------------------------------------------------------------------

function Private.normalize_path(path)
  if type(path) ~= "string" then
    return ""
  end
  local trimmed = (path:gsub("^%s+", ""):gsub("%s+$", "")):gsub("^%./", "")
  return trimmed
end

function Private.clone_list(items)
  local copy = {}
  for _, item in ipairs(items or {}) do
    table.insert(copy, item)
  end
  return copy
end

function Private.sort_threads(threads)
  table.sort(threads, function(a, b)
    if a.path == b.path then
      if a.line == b.line then
        return a.id < b.id
      end
      return a.line < b.line
    end
    return a.path < b.path
  end)
end

function Private.map_thread(threads, thread_id, transform)
  local next_threads = {}
  local found = false
  for _, thread in ipairs(threads or {}) do
    if thread.id == thread_id then
      found = true
      table.insert(next_threads, transform(thread))
    else
      table.insert(next_threads, thread)
    end
  end
  assert(found, "thread not found: " .. tostring(thread_id))
  return next_threads
end

return LocalReviewService
