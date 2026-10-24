-- State -----------------------------------------------------------------------
local M = {}
local store = require("lgtm.services.state")
local rpc = require("lgtm.services.client")

-- Public ----------------------------------------------------------------------
-- Conversations ---------------------------------------------------------------

---@param state lgtm.ReviewState
function M.get_conversation(state, callback)
  if state.conversation_read then
    state.conversation_read.callbacks[#state.conversation_read.callbacks + 1] = callback
    state.conversation_read.again = true
    return
  end
  local reader = { callbacks = { callback } }
  state.conversation_read = reader
  local result
  local function finish(err)
    state.conversation_read = nil
    if not err then
      state.version = result.version
      state.conversation = result
    end
    for _, done in ipairs(reader.callbacks) do
      done(err, not err and result or nil)
    end
  end
  local page
  local function restart()
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
function M.refresh(state, callback)
  local unavailable = store.get_readiness_error(state)
  if unavailable then
    if callback then
      callback(unavailable)
    end
    return
  end
  local function read()
    M.get_conversation(state, function(err)
      if state.review and store.current ~= state then
        if callback then
          callback({ message = "Review changed while loading" })
        end
        return
      end
      if not err and state.render then
        state.render()
      end
      if not err and state.observe_sources then
        state.observe_sources(function(observe_error)
          if callback then
            callback(observe_error)
          elseif observe_error then
            if state.on_error then
              state.on_error(observe_error)
            end
          end
        end)
      elseif callback then
        callback(err)
      elseif err then
        if state.on_error then
          state.on_error(err)
        end
      end
    end)
  end
  if state.review then
    rpc.call("GetReview", { reviewId = state.review.id }, function(err, response)
      if err then
        if callback then
          callback(err)
        else
          if state.on_error then
            state.on_error(err)
          end
        end
      else
        store.apply(state, response.review)
        read()
      end
    end)
  else
    read()
  end
end

-- Review lifecycle ------------------------------------------------------------

---@param state lgtm.ReviewState
function M.ensure_writable(state, callback)
  if state.kind ~= "pr" or not state.review or state.review.state ~= "completed" then
    callback()
    return
  end
  if state.review.submissionStatus ~= "confirmed" then
    callback({ message = "Reconcile the previous submission before adding another draft" })
    return
  end
  if state.continuing_review then
    state.continuing_review[#state.continuing_review + 1] = callback
    return
  end
  state.continuing_review = { callback }
  state.continuation_key = state.continuation_key or store.request_key()
  rpc.call("StartReview", {
    target = state.target,
    requestKey = state.continuation_key,
    resumePolicy = "RESUME_POLICY_MATCHING_ONLY",
  }, function(err, response)
    if not err and store.current ~= state then
      err = { message = "Review changed while preparing the next draft" }
    end
    if not err and response.review.state ~= "active" then
      err = { message = "The next PR review is not writable" }
    end
    local function finish(failure)
      local waiting = state.continuing_review
      state.continuing_review = nil
      for _, done in ipairs(waiting) do
        done(failure)
      end
    end
    if err then
      finish(err)
      return
    end
    store.apply(state, response.review)
    if state.review.id ~= response.review.id or state.review.state ~= "active" then
      finish({ message = "Review changed while preparing the next draft" })
      return
    end
    state.continuation_key = nil
    -- Starting an attempt can update thread versions while importing the PR discussion.
    M.get_conversation(state, finish)
  end)
end

function M.start(target, comparison, root, callback, options)
  local on_conflict = options and options.on_conflict
  local input = vim.tbl_extend("force", {
    target = target,
    comparison = comparison,
    requestKey = store.request_key(),
    resumePolicy = "RESUME_POLICY_MATCHING_ONLY",
  }, options or {})
  input.on_conflict = nil
  rpc.call("StartReview", input, function(err, response)
    if not err then
      callback(nil, store.apply({ root = root }, response.review))
      return
    end
    if (not target.localGit and not target.pullRequest) or err.code ~= "failed_precondition" then
      callback(err)
      return
    end
    rpc.call("ListReviews", { target = target, state = "active" }, function(list_error, saved)
      local review = saved and saved.reviews and saved.reviews[1]
      if list_error or not review then
        callback(list_error or err)
        return
      end
      if target.pullRequest then
        -- A picker comparison can age while StartReview observes a push.
        -- Retry selection once using the server's current PR comparison.
        local retry = vim.tbl_extend("force", input, { requestKey = store.request_key() })
        retry.comparison = nil
        rpc.call("StartReview", retry, function(retry_error, current)
          if retry_error then
            callback(retry_error)
          else
            callback(nil, store.apply({ root = root }, current.review))
          end
        end)
        return
      end
      if not on_conflict then
        callback({ message = "Saved review uses a different comparison", review = review })
        return
      end
      on_conflict(review, function(choice)
        if choice == "Resume saved review" then
          callback(nil, store.apply({ root = root }, review))
        else
          input.resumePolicy = "RESUME_POLICY_REPLACE_LOCAL"
          input.expectedReviewId = review.id
          input.expectedVersion = review.version
          input.requestKey = store.request_key()
          rpc.call("StartReview", input, function(replace_error, replacement)
            callback(
              replace_error,
              not replace_error and store.apply({ root = root }, replacement.review) or nil
            )
          end)
        end
      end)
    end)
  end)
end

---@param state lgtm.ReviewState
function M.refresh_comparison(state, callback)
  rpc.call("RefreshReview", {
    scope = state.scope,
    expectedVersion = state.version,
    requestKey = store.request_key(),
  }, function(err, result)
    if not err and result.review then
      store.apply(state, result.review)
    end
    callback(err, state)
  end)
end

---@param state lgtm.ReviewState
function M.complete(state, callback)
  rpc.call("CompleteReview", {
    reviewId = state.review.id,
    expectedVersion = state.version,
    requestKey = store.request_key(),
  }, function(err, result)
    if not err then
      store.apply(state, result.review)
    end
    callback(err, state)
  end)
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
      state.refresh(callback)
      return
    end
    rpc.call(
      "SyncDraftComment",
      { reviewId = state.review.id, commentId = drafts[index].id },
      function(err)
        if err then
          state.refresh(function()
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

-- Export ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.get_export(state, callback)
  rpc.call("Export", { scope = state.scope }, function(err, result)
    callback(err, result and result.markdown)
  end)
end

-- Saved locations -------------------------------------------------------------

---@param state lgtm.ReviewState
function M.record_location(state, callback, can_rebase)
  local pending = state.pending_location
  rpc.call("RefreshReview", pending.input, function(err, result)
    if
      err
      and can_rebase ~= false
      and err.code == "failed_precondition"
      and (err.message or ""):find("Review data changed", 1, true)
    then
      M.get_conversation(state, function(read_error)
        if read_error then
          callback(read_error)
          return
        end
        pending.input.expectedVersion, pending.input.requestKey = state.version, store.request_key()
        M.record_location(state, callback, false)
      end)
    else
      callback(err, result)
    end
  end)
end

return M
