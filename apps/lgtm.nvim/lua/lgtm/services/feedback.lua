-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")
local store = require("lgtm.services.state")
local reviews = require("lgtm.services.reviews")

---@param state lgtm.ReviewState
---@param draft lgtm.Draft
-- Public ----------------------------------------------------------------------

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
      state.refresh(function(refresh_error)
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
    target.refresh(function(refresh_error)
      callback(refresh_error, result)
    end)
  end)
end

---@param state lgtm.ReviewState
function M.mutate(state, method, input, callback)
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
  state.pending_mutations = state.pending_mutations or {}
  local fingerprint = method
    .. ":"
    .. vim.json.encode({
      scope = input.scope,
      commentId = input.commentId,
      threadId = input.threadId,
      body = input.body,
      resolved = input.resolved,
    })
  if state.pending_mutations[fingerprint] then
    input = state.pending_mutations[fingerprint]
  else
    input.requestKey = store.request_key()
    state.pending_mutations[fingerprint] = input
  end
  state.mutation_in_flight = state.mutation_in_flight or {}
  if state.mutation_in_flight[fingerprint] then
    return
  end
  state.mutation_in_flight[fingerprint] = true
  rpc.call(method, input, function(err, result)
    state.mutation_in_flight[fingerprint] = nil
    if err then
      if err.message and err.message:find("Request rejected:", 1, true) then
        state.pending_mutations[fingerprint] = nil
      end
      callback(err)
      return
    end
    state.pending_mutations[fingerprint] = nil
    local applied = method == "DeleteComment" or method == "ResolveThread"
    if method == "DeleteComment" then
      state.conversation.comments = vim.tbl_filter(function(comment)
        return comment.id ~= input.commentId
      end, state.conversation.comments)
      state.conversation.threads = vim.tbl_filter(function(thread)
        return vim.iter(state.conversation.comments):any(function(comment)
          return comment.threadId == thread.id
        end)
      end, state.conversation.threads)
    elseif method == "ResolveThread" and result.thread then
      for index, thread in ipairs(state.conversation.threads) do
        if thread.id == result.thread.id then
          state.conversation.threads[index] = result.thread
        end
      end
    end
    if applied then
      if state.render then
        state.render()
      end
      callback(nil, result)
    end
    state.refresh(function(refresh_error)
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

return M
