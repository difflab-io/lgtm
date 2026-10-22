-- Consumer state contains a review or saved-file scope; storage setup belongs to the backend.
local M = { current = nil, generation = 0, tabs = {} }
local rpc = require("lgtm.rpc")

function M.set(state)
  if M.current ~= state then
    M.generation = M.generation + 1
    M.current = state
  end
end

function M.request_key()
  return vim.fn.sha256(tostring(vim.uv.hrtime()) .. tostring(math.random()))
end

-- A native diff may be visible before its persisted review is ready.
function M.readiness_error(state)
  if state.load_error then
    local cause = type(state.load_error) == "table" and state.load_error.message
      or tostring(state.load_error)
    return {
      message = "PR review could not load: "
        .. (cause or "Unknown error")
        .. ". Use :Lgtm refresh to retry.",
    }
  end
  if state.initializing then
    return { message = "PR comments are still loading; wait for Review ready" }
  end
end

function M.apply(state, review)
  if
    state.version
    and tonumber(review.version)
    and tonumber(state.version)
    and tonumber(review.version) < tonumber(state.version)
  then
    return state
  end
  local comparison_changed = not vim.deep_equal(state.comparison, review.comparison)
  local load_progress = state.reviewed_review ~= review.id or comparison_changed
  if state.kind == "pr" and state.comparison and comparison_changed then
    state.views, state.reviewed, state.observe_sources = {}, {}, nil
    state.source_generation = (state.source_generation or 0) + 1
    state.render_ready = false
  end
  state.review = review
  state.scope = { reviewId = review.id }
  state.target = review.target
  state.comparison = review.comparison
  state.version = review.version
  for kind, comparison in pairs(review.comparison or {}) do
    state.mode = kind == "pullRequest" and "pr" or kind
    state.base, state.head = comparison.baseCommit, comparison.headCommit
  end
  state.kind = review.target.pullRequest and "pr"
    or review.target.directory and "directory"
    or "git"
  state.root = review.target.directory and review.target.directory.path
    or state.root
    or (review.target.localGit or {}).path
  state.scope_key = review.id
  if load_progress then
    require("lgtm.files").load_reviewed(state)
  end
  return state
end

-- Finite pages bound to one server version. A read never starts or refreshes a review.
function M.conversation(state, callback)
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

function M.refresh(state, callback)
  local unavailable = M.readiness_error(state)
  if unavailable then
    if callback then
      callback(unavailable)
    end
    return
  end
  local function read()
    M.conversation(state, function(err)
      if state.review and M.current ~= state then
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
            require("lgtm.notifications").error(observe_error)
          end
        end)
      elseif callback then
        callback(err)
      elseif err then
        require("lgtm.notifications").error(err)
      end
    end)
  end
  if state.review then
    rpc.call("GetReview", { reviewId = state.review.id }, function(err, response)
      if err then
        if callback then
          callback(err)
        else
          require("lgtm.notifications").error(err)
        end
      else
        M.apply(state, response.review)
        read()
      end
    end)
  else
    read()
  end
end

function M.start(target, comparison, root, callback, options)
  local input = vim.tbl_extend("force", {
    target = target,
    comparison = comparison,
    requestKey = M.request_key(),
    resumePolicy = "RESUME_POLICY_MATCHING_ONLY",
  }, options or {})
  rpc.call("StartReview", input, function(err, response)
    if not err then
      callback(nil, M.apply({ root = root }, response.review))
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
        local retry = vim.tbl_extend("force", input, { requestKey = M.request_key() })
        retry.comparison = nil
        rpc.call("StartReview", retry, function(retry_error, current)
          if retry_error then
            callback(retry_error)
          else
            callback(nil, M.apply({ root = root }, current.review))
          end
        end)
        return
      end
      require("lgtm.picker").select(
        "Saved review uses a different comparison",
        {
          "Resume saved review",
          "Complete saved review and start this comparison",
        },
        tostring,
        function(choice)
          if choice == "Resume saved review" then
            callback(nil, M.apply({ root = root }, review))
          else
            input.resumePolicy = "RESUME_POLICY_REPLACE_LOCAL"
            input.expectedReviewId = review.id
            input.expectedVersion = review.version
            input.requestKey = M.request_key()
            rpc.call("StartReview", input, function(replace_error, replacement)
              callback(
                replace_error,
                not replace_error and M.apply({ root = root }, replacement.review) or nil
              )
            end)
          end
        end
      )
    end)
  end)
end

function M.notes(path)
  local absolute = vim.fn.fnamemodify(path, ":p"):gsub("/$", "")
  absolute = vim.uv.fs_realpath(absolute) or absolute
  return {
    note = true,
    kind = "notes",
    mode = "notes",
    root = vim.fs.dirname(absolute),
    path = vim.fs.basename(absolute),
    scope = { notes = { path = absolute } },
    scope_key = "notes:" .. absolute,
  }
end

return M
