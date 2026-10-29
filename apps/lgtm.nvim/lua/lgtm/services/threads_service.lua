-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")
local store = require("lgtm.services.state")
local reviews = require("lgtm.services.session_service")

-- Application data belongs to this service. ReviewState exposes read-only projections.
local records = setmetatable({}, { __mode = "k" })
local Private = {}

-- Methods ---------------------------------------------------------------------

---@return NuiSignalValue
function M.get_data(state)
  local record = records[state]
  if not record then
    local signal = require("nui-components.signal").create({ value = {} })
    record = { signal = signal, views = rawget(state, "placement_views") }
    records[state] = record
    signal.value = {
      conversation = rawget(state, "conversation"),
      placements = rawget(state, "placements") or {},
      loading = false,
      saving = false,
      error = false,
    }
    for _, key in ipairs({
      "conversation",
      "placements",
      "placement_views",
      "placement_conversation",
    }) do
      rawset(state, key, nil)
    end
    setmetatable(state, {
      __index = function(_, key)
        if key == "placement_views" then
          return record.views
        end
        local snapshot = record.signal:get_value().value
        if key == "placement_conversation" then
          return snapshot.conversation
        end
        if key == "conversation" or key == "placements" then
          return snapshot[key]
        end
      end,
      __newindex = function(target, key, value)
        if
          key == "conversation"
          or key == "placements"
          or key == "placement_views"
          or key == "placement_conversation"
        then
          error("Conversation data is owned by threads_service")
        end
        rawset(target, key, value)
      end,
    })
  end
  return record.signal.value
end

function M.publish(state, conversation, placements, views)
  local current = M.get_data(state):get_value()
  records[state].views = views
  records[state].signal.value = vim.tbl_extend("force", current, {
    conversation = conversation,
    placements = placements or {},
  })
end

---@param state lgtm.ReviewState
function M.get_conversation(state, callback)
  M.get_data(state)
  Private.status(state, true)
  if records[state].reader then
    records[state].reader.callbacks[#records[state].reader.callbacks + 1] = callback
    records[state].reader.again = true
    return
  end
  local reader = { callbacks = { callback } }
  records[state].reader = reader
  local result
  local page, restart
  local function finish(err)
    local function notify(error)
      if reader.again then
        restart()
        return
      end
      records[state].reader = nil
      Private.status(state, false, error)
      if not error then
        state.version = result.version
      end
      for _, done in ipairs(reader.callbacks) do
        done(error, not error and result or nil)
      end
    end
    if err then
      notify(err)
    else
      require("lgtm.services.anchor_service").get_placements(state, notify, result)
    end
  end
  restart = function()
    reader.again = false
    result = { threads = {}, comments = {} }
    page(nil)
  end
  page = function(token)
    rpc.call(
      "GetConversation",
      { scope = state.scope, includeDrafts = true, pageSize = 100, pageToken = token },
      function(err, response)
        -- A caller arrived after this read began (for example, after saving a note).
        -- Complete all waiters with one fresh snapshot; never leave a composer waiting on a discarded read.
        if reader.again then
          restart()
          return
        end
        if err then
          finish(err)
          return
        end
        if result.version and result.version ~= response.version then
          finish({ message = "Conversation changed while loading. Refresh to try again." })
          return
        end
        result.version = response.version
        for _, thread in ipairs(response.threads or {}) do
          if
            not vim.iter(result.threads):any(function(saved)
              return saved.id == thread.id
            end)
          then
            result.threads[#result.threads + 1] = thread
          end
        end
        vim.list_extend(result.comments, response.comments or {})
        if response.nextPageToken and response.nextPageToken ~= "" then
          page(response.nextPageToken)
        else
          finish()
        end
      end
    )
  end
  restart()
end

---@param state lgtm.ReviewState
function M.observe_sources(state, file, callback)
  M.get_data(state)
  callback = callback or function() end
  if records[state].observing then
    records[state].observe_pending = true
    callback()
    return
  end
  if
    state.kind == "pr"
    or state.initializing
    or state.load_error
    or not state.conversation
    or #(state.conversation.comments or {}) == 0
  then
    callback()
    return
  end
  local selected = state.views and state.views.new
  if not selected or selected.unavailable then
    callback()
    return
  end
  records[state].observing = true
  rpc.call("RefreshReview", {
    scope = state.scope,
    expectedVersion = state.version,
    observePath = file.path,
    requestKey = store.request_key(),
  }, function(err, result)
    if not err then
      if result.review then
        store.apply(state, result.review)
      else
        state.version = result.version
      end
    end
    -- Read accepted state before another observation can use its version.
    M.get_conversation(state, function(read_error)
      records[state].observing = false
      callback(err or read_error)
      local pending = records[state].observe_pending
      records[state].observe_pending = nil
      if pending and not err and not read_error and state.observe_sources then
        vim.schedule(state.observe_sources)
      end
    end)
  end)
end

---@param state lgtm.ReviewState
---@return lgtm.Draft
function M.create_draft(state, anchor)
  return {
    scope = state.scope,
    expectedVersion = state.version,
    requestKey = store.request_key(),
    anchor = anchor,
  }
end

---@param state lgtm.ReviewState
function M.save(state, draft, body, callback)
  reviews.ensure_writable(state, function(err)
    if err then
      callback(err)
      return
    end
    if draft.body and draft.body ~= body then
      draft.requestKey = store.request_key()
    end
    if not draft.body then
      draft.scope = vim.deepcopy(state.scope)
      draft.expectedVersion = state.version
    end
    draft.body = body
    rpc.call("AddComment", draft, function(error, result)
      if error then
        callback(error)
        return
      end
      reviews.refresh(state, function(refresh_error)
        callback(refresh_error, result)
      end)
    end)
  end)
end

function M.promote(source, target, comment, anchor, callback)
  rpc.call("PromoteNotes", {
    source = source.scope,
    targetReviewId = target.review.id,
    expectedSourceVersion = source.version,
    expectedReviewVersion = target.version,
    commentIds = { comment.id },
    anchors = { anchor },
    requestKey = store.request_key(),
  }, function(err, result)
    if err then
      callback(err)
      return
    end
    reviews.refresh(target, function(refresh_error)
      callback(refresh_error, result)
    end)
  end)
end

---@param state lgtm.ReviewState
function M.mutate(state, method, input, callback)
  M.get_data(state)
  local unavailable = store.get_readiness_error(state)
  if unavailable then
    callback(unavailable)
    return
  end
  if method == "Reply" and state.kind == "pr" and state.review.state == "completed" then
    reviews.ensure_writable(state, function(err)
      if err then
        callback(err)
      else
        M.mutate(state, method, input, callback)
      end
    end)
    return
  end
  input.scope = state.scope
  local object
  for _, comment in ipairs((state.conversation or {}).comments or {}) do
    if comment.id == input.commentId then
      object = comment
    end
  end
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    if thread.id == input.threadId then
      object = thread
    end
  end
  input.expectedVersion = object and object.version or state.version
  records[state].pending = records[state].pending or {}
  local fingerprint = method
    .. ":"
    .. vim.json.encode({
      scope = input.scope,
      commentId = input.commentId,
      threadId = input.threadId,
      body = input.body,
      resolved = input.resolved,
    })
  if records[state].pending[fingerprint] then
    input = records[state].pending[fingerprint]
  else
    input.requestKey = store.request_key()
    records[state].pending[fingerprint] = input
  end
  records[state].in_flight = records[state].in_flight or {}
  if records[state].in_flight[fingerprint] then
    return
  end
  records[state].in_flight[fingerprint] = true
  Private.operation_status(state)
  rpc.call(method, input, function(err, result)
    records[state].in_flight[fingerprint] = nil
    Private.operation_status(state, err)
    if err then
      if err.message and err.message:find("Request rejected:", 1, true) then
        records[state].pending[fingerprint] = nil
      end
      callback(err)
      return
    end
    records[state].pending[fingerprint] = nil
    local applied = method == "DeleteComment" or method == "ResolveThread"
    local conversation = vim.deepcopy(state.conversation)
    if method == "DeleteComment" then
      conversation.comments = vim.tbl_filter(function(comment)
        return comment.id ~= input.commentId
      end, conversation.comments)
      conversation.threads = vim.tbl_filter(function(thread)
        return vim.iter(conversation.comments):any(function(comment)
          return comment.threadId == thread.id
        end)
      end, conversation.threads)
    elseif method == "ResolveThread" and result.thread then
      for index, thread in ipairs(conversation.threads) do
        if thread.id == result.thread.id then
          conversation.threads[index] = result.thread
        end
      end
    end
    if applied then
      M.publish(state, conversation, state.placements, state.placement_views)
      callback(nil, result)
    end
    reviews.refresh(state, function(refresh_error)
      if applied then
        if refresh_error and state.on_error then
          state.on_error(refresh_error)
        end
      else
        callback(refresh_error, result)
      end
    end)
  end)
end

function M.reply(state, thread_id, body, callback)
  M.mutate(state, "Reply", { threadId = thread_id, body = body }, callback)
end

function M.edit(state, comment_id, body, callback)
  M.mutate(state, "EditComment", { commentId = comment_id, body = body }, callback)
end

function M.delete(state, comment_id, callback)
  M.mutate(state, "DeleteComment", { commentId = comment_id }, callback)
end

function M.resolve(state, thread_id, resolved, callback)
  for _, thread in ipairs((state.conversation or {}).threads or {}) do
    if thread.id == thread_id and not M.can_resolve(state, thread) then
      callback({ message = "Publish draft comments before resolving their thread" })
      return
    end
  end
  M.mutate(state, "ResolveThread", { threadId = thread_id, resolved = resolved }, callback)
end

function M.can_edit(comment)
  local owner = comment.ownerId
  if not owner or owner == "" then
    owner = comment.author and comment.author.id
  end
  return owner == rpc.identity
end

---@param state lgtm.ReviewState
function M.can_resolve(state, thread)
  if state.kind ~= "pr" then
    return true
  end
  for _, comment in ipairs((state.conversation or {}).comments or {}) do
    if comment.threadId == thread.id and comment.visibility ~= "draft" then
      return true
    end
  end
  return false
end

-- Draft synchronization -------------------------------------------------------

---@param state lgtm.ReviewState
function M.sync_drafts(state, callback)
  local drafts = vim.tbl_filter(function(comment)
    local owner = comment.ownerId ~= "" and comment.ownerId or (comment.author or {}).id
    return comment.originSessionId == state.review.id
      and owner == rpc.identity
      and comment.visibility == "draft"
  end, (state.conversation or {}).comments or {})
  table.sort(drafts, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  local index = 0
  local function next_draft()
    index = index + 1
    if not drafts[index] then
      reviews.refresh(state, callback)
      return
    end
    rpc.call(
      "SyncDraftComment",
      { reviewId = state.review.id, commentId = drafts[index].id },
      function(err)
        if err then
          reviews.refresh(state, function()
            callback(err)
          end)
        else
          next_draft()
        end
      end
    )
  end
  next_draft()
end

function M.get_comments(state, thread)
  local result = {}
  for _, c in ipairs(state.conversation.comments or {}) do
    if c.threadId == thread.id then
      result[#result + 1] = c
    end
  end
  table.sort(result, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  return result
end

-- Private ---------------------------------------------------------------------

function Private.operation_status(state, err)
  local current = M.get_data(state):get_value()
  records[state].signal.value = vim.tbl_extend("force", current, {
    saving = next(records[state].in_flight or {}) ~= nil,
    error = err or false,
  })
end

function Private.status(state, loading, err)
  local current = M.get_data(state):get_value()
  records[state].signal.value =
    vim.tbl_extend("force", current, { loading = loading, error = err or false })
end
return M
