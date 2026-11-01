---@class LgtmComment
---@field id string
---@field thread_id string
---@field author string
---@field body string
---@field state "DRAFT"|"PENDING"|"PUBLISHED"
---@field created_at string
---@field updated_at string

---@class LgtmThread
---@field id string
---@field mode "local"|"pr"
---@field pr_number integer|nil
---@field path string
---@field side "LEFT"|"RIGHT"
---@field start_line integer
---@field line integer
---@field resolved boolean
---@field outdated boolean
---@field collapsed boolean
---@field comments LgtmComment[]

local Thread = {}
local Private = {}

-- Methods ---------------------------------------------------------------------

---Construct a normalized comment entity.
---@param input table
---@return LgtmComment
function Thread.new_comment(input)
  assert(type(input) == "table", "comment input must be a table")
  local body = Private.trim(input.body)
  assert(body ~= "", "comment body is required")
  local thread_id = assert(input.thread_id, "comment thread_id is required")
  local now = input.created_at or Private.iso_now()
  local state = input.state or "DRAFT"
  assert(
    state == "DRAFT" or state == "PENDING" or state == "PUBLISHED",
    "invalid comment state: " .. tostring(state)
  )
  return {
    id = input.id or Private.next_id("comment"),
    thread_id = tostring(thread_id),
    author = Private.trim(input.author) ~= "" and Private.trim(input.author) or "you",
    body = body,
    state = state,
    created_at = now,
    updated_at = input.updated_at or now,
  }
end

---Construct a normalized review thread entity with at least one comment or initial body.
---@param input table
---@return LgtmThread
function Thread.new(input)
  assert(type(input) == "table", "thread input must be a table")
  local path = Private.normalize_path(input.path)
  assert(path ~= "", "thread path is required")
  local line = tonumber(input.line)
  assert(line and line >= 1, "thread line must be >= 1")
  local start_line = tonumber(input.start_line) or line
  if start_line > line then
    start_line, line = line, start_line
  end
  local mode = input.mode or "local"
  assert(mode == "local" or mode == "pr", "invalid thread mode: " .. tostring(mode))
  local side = input.side or "RIGHT"
  assert(side == "LEFT" or side == "RIGHT", "invalid thread side: " .. tostring(side))
  local thread_id = tostring(input.id or Private.next_id("thread"))
  local comments = Private.build_comments(thread_id, mode, input)
  assert(#comments >= 1, "thread must contain at least one comment")

  return {
    id = thread_id,
    mode = mode,
    pr_number = input.pr_number and tonumber(input.pr_number) or nil,
    path = path,
    side = side,
    start_line = start_line,
    line = line,
    resolved = input.resolved == true,
    outdated = input.outdated == true,
    collapsed = input.collapsed == true,
    comments = comments,
  }
end

---Return a shallow copy of the thread with updated resolved status.
---@param thread LgtmThread
---@param resolved boolean
---@return LgtmThread
function Thread.with_resolved(thread, resolved)
  local copy = Private.copy_thread(thread)
  copy.resolved = resolved == true
  return copy
end

---Return a shallow copy of the thread with updated collapsed status.
---@param thread LgtmThread
---@param collapsed boolean
---@return LgtmThread
function Thread.with_collapsed(thread, collapsed)
  local copy = Private.copy_thread(thread)
  copy.collapsed = collapsed == true
  return copy
end

---Return a copy of the thread with a new reply appended.
---@param thread LgtmThread
---@param reply_input table|string
---@return LgtmThread, LgtmComment
function Thread.with_reply(thread, reply_input)
  local copy = Private.copy_thread(thread)
  local payload = type(reply_input) == "string" and { body = reply_input }
    or vim.tbl_extend("force", {}, reply_input or {})
  payload.thread_id = copy.id
  payload.state = payload.state or (copy.mode == "pr" and "PENDING" or "DRAFT")
  local comment = Thread.new_comment(payload)
  table.insert(copy.comments, comment)
  return copy, comment
end

---Return a copy of the thread with the target comment body updated.
---@param thread LgtmThread
---@param comment_id string
---@param new_body string
---@return LgtmThread, LgtmComment
function Thread.with_edited_comment(thread, comment_id, new_body)
  local body = Private.trim(new_body)
  assert(body ~= "", "edited comment body is required")
  local copy = Private.copy_thread(thread)
  local updated = nil
  for index, comment in ipairs(copy.comments) do
    if comment.id == comment_id then
      updated = vim.tbl_extend("force", {}, comment, {
        body = body,
        updated_at = Private.iso_now(),
      })
      copy.comments[index] = updated
      break
    end
  end
  assert(updated ~= nil, "comment not found: " .. tostring(comment_id))
  return copy, updated
end

---Return a copy of the thread without the target comment, or nil if the last comment was removed.
---@param thread LgtmThread
---@param comment_id string
---@return LgtmThread|nil
function Thread.without_comment(thread, comment_id)
  local copy = Private.copy_thread(thread)
  local kept = {}
  local found = false
  for _, comment in ipairs(copy.comments) do
    if comment.id == comment_id then
      found = true
    else
      table.insert(kept, comment)
    end
  end
  assert(found, "comment not found: " .. tostring(comment_id))
  if #kept == 0 then
    return nil
  end
  copy.comments = kept
  return copy
end

---Return the root comment and reply count for a thread.
---@param thread LgtmThread
---@return LgtmComment, integer
function Thread.summary_stats(thread)
  local root = thread.comments[1]
  local reply_count = math.max(0, #thread.comments - 1)
  return root, reply_count
end

---Format a human-readable file and line range label for a thread.
---@param thread LgtmThread
---@return string
function Thread.format_anchor(thread)
  if thread.start_line and thread.start_line < thread.line then
    return string.format("%s:%d-%d", thread.path, thread.start_line, thread.line)
  end
  return string.format("%s:%d", thread.path, thread.line)
end

-- Private ---------------------------------------------------------------------

local sequence = 0

function Private.next_id(prefix)
  sequence = sequence + 1
  return string.format("%s-%d", prefix, sequence)
end

function Private.iso_now()
  return os.date("!%Y-%m-%dT%H:%M:%SZ")
end

function Private.trim(value)
  if type(value) ~= "string" then
    return ""
  end
  return (value:gsub("^%s+", ""):gsub("%s+$", ""))
end

function Private.normalize_path(path)
  local trimmed = Private.trim(path)
  trimmed = trimmed:gsub("^%./", "")
  return trimmed
end

function Private.build_comments(thread_id, mode, input)
  local list = {}
  if type(input.comments) == "table" and #input.comments > 0 then
    for _, raw in ipairs(input.comments) do
      local comment_input = vim.tbl_extend("force", {}, raw, {
        thread_id = thread_id,
        state = raw.state or (mode == "pr" and "PENDING" or "DRAFT"),
      })
      table.insert(list, Thread.new_comment(comment_input))
    end
    return list
  end
  if Private.trim(input.body) ~= "" then
    table.insert(
      list,
      Thread.new_comment({
        id = input.comment_id,
        thread_id = thread_id,
        author = input.author,
        body = input.body,
        state = input.state or (mode == "pr" and "PENDING" or "DRAFT"),
        created_at = input.created_at,
        updated_at = input.updated_at,
      })
    )
  end
  return list
end

function Private.copy_thread(thread)
  local comments = {}
  for _, comment in ipairs(thread.comments or {}) do
    table.insert(comments, vim.tbl_extend("force", {}, comment))
  end
  return vim.tbl_extend("force", {}, thread, {
    comments = comments,
  })
end

return Thread
