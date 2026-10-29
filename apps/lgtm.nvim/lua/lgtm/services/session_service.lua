-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local store = require("lgtm.services.state")
local rpc = require("lgtm.services.client")

local records = setmetatable({}, { __mode = "k" })
local catalog

-- Methods ---------------------------------------------------------------------

---@return NuiSignalValue
function M.get_data(state)
  if not records[state] then
    require("lgtm.services.threads_service").get_data(state)
    local signal = require("nui-components.signal").create({ value = {} })
    signal.value = {
      review = rawget(state, "review"),
      loading = state.initializing == true,
      error = state.load_error or false,
    }
    records[state] = signal
    rawset(state, "review", nil)
    rawset(state, "initializing", nil)
    rawset(state, "load_error", nil)
    local previous = getmetatable(state)
    setmetatable(state, {
      __index = function(target, key)
        local data = signal:get_value().value
        if key == "review" then
          return data.review
        end
        if key == "initializing" then
          return data.loading
        end
        if key == "load_error" then
          return data.error or nil
        end
        return previous.__index(target, key)
      end,
      __newindex = function(target, key, value)
        if key == "review" or key == "initializing" or key == "load_error" then
          error("Review data is owned by session_service")
        end
        previous.__newindex(target, key, value)
      end,
    })
  end
  return records[state].value
end

function M.set_review(state, review)
  local current = M.get_data(state):get_value()
  records[state].value = vim.tbl_extend("force", current, { review = review })
end

function M.set_status(state, loading, err)
  local current = M.get_data(state):get_value()
  records[state].value =
    vim.tbl_extend("force", current, { loading = loading, error = err or false })
end

---@return NuiSignalValue
function M.get_catalog()
  if not catalog then
    catalog = require("nui-components.signal").create({
      value = { reviews = {}, pull_requests = {}, loading = false, error = false },
    })
  end
  return catalog.value
end

-- Conversations ---------------------------------------------------------------

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
    require("lgtm.services.threads_service").get_conversation(state, function(err)
      if state.review and store.current ~= state then
        if callback then
          callback({ message = "Review changed while loading" })
        end
        return
      end
      if callback then
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
      local waiting = state.continuing_review or {}
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
    require("lgtm.services.threads_service").get_conversation(state, finish)
  end)
end

function M.create(target, comparison, root, callback, options)
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
function M.select_revision(state, revision_id, callback)
  state.selected_revision_id = revision_id
  M.refresh(state, function(err)
    if err then
      if state.on_error then
        state.on_error(err)
      end
      return
    end
    require("lgtm.app").refresh_view(state)
    local previous = state.on_ready
    state.on_ready = function()
      state.on_ready = previous
      if previous then
        previous()
      end
      callback()
    end
  end)
end

function M.refresh_comparison(state, callback)
  state.selected_revision_id = nil
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
function M.close(state, callback)
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

-- Export ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.get_export(state, callback)
  rpc.call("Export", { scope = state.scope }, function(err, result)
    callback(err, result and result.markdown)
  end)
end

function M.get_sessions(completed, callback)
  Private.catalog_status(true)
  catalog.value = vim.tbl_extend("force", M.get_catalog():get_value(), { reviews = {} })
  local reviews = {}
  local function page(token)
    rpc.latest(
      "review-picker",
      "ListReviews",
      { state = completed and "completed" or "active", pageToken = token, pageSize = 100 },
      function(err, result)
        if err then
          Private.catalog_status(false, err)
          if callback then
            callback(err)
          end
          return
        end
        vim.list_extend(reviews, result.reviews or {})
        if result.nextPageToken and result.nextPageToken ~= "" then
          page(result.nextPageToken)
        else
          catalog.value = vim.tbl_extend(
            "force",
            M.get_catalog():get_value(),
            { reviews = reviews, loading = false, error = false }
          )
          if callback then
            callback(nil, reviews)
          end
        end
      end
    )
  end
  page(nil)
end

function M.get_pull_requests(url, query, page, callback)
  Private.catalog_status(true)
  catalog.value = vim.tbl_extend("force", M.get_catalog():get_value(), { pull_requests = {} })
  rpc.latest(
    "pr-picker",
    "ListPullRequests",
    { repositoryUrl = url, query = query, page = page },
    function(err, result)
      if err then
        Private.catalog_status(false, err)
        if callback then
          callback(err)
        end
        return
      end
      local entries = result.pullRequests or {}
      if (result.nextPage or 0) > 0 then
        entries[#entries + 1] = { nextPage = result.nextPage }
      end
      catalog.value = vim.tbl_extend(
        "force",
        M.get_catalog():get_value(),
        { pull_requests = entries, loading = false, error = false }
      )
      if callback then
        callback(nil, entries)
      end
    end
  )
end

---@param state lgtm.ReviewState
---@return lgtm.Draft
function M.get_draft(state)
  local draft = store.submissions[state.review.id]
  if not draft then
    draft = {
      body = state.review.body or "",
      requestKey = store.request_key(),
    }
    store.submissions[state.review.id] = draft
  end
  return draft
end

---@param state lgtm.ReviewState
---@return lgtm.ServiceError?
function M.get_submission_error(state)
  local unavailable = store.get_readiness_error(state)
  if unavailable then
    return unavailable
  end
  if state.selected_revision_id and state.snapshot.id ~= state.review.snapshot.id then
    return { message = "Return to the latest revision with :Lgtm refresh before submitting" }
  end
  local status = state.review.submissionStatus
  local draft = store.submissions[state.review.id]
  if draft and draft.uncertain and status ~= "failed" then
    return { message = "Submission response was lost. Use :Lgtm reconcile before retrying." }
  end
  if (status or ""):match("^draft_") then
    return { message = "Draft synchronization needs recovery. Use :Lgtm sync before submitting." }
  end
  if vim.list_contains({ "uncertain", "pending", "posting", "sending", "reconciling" }, status) then
    return { message = "Submission is pending or uncertain. Use :Lgtm reconcile before retrying." }
  end
  local recovery_error = Private.get_recovery_error(state)
  if recovery_error then
    return recovery_error
  end
  if state.kind ~= "pr" then
    return { message = "Local reviews cannot be submitted to GitHub; use :Lgtm complete" }
  end
end

---@param state lgtm.ReviewState
---@param draft lgtm.Draft
function M.submit(state, draft, outcome, body, callback)
  draft.body, draft.outcome = body, outcome
  local input = {
    reviewId = state.review.id,
    outcome = outcome,
    body = body,
    expectedHead = state.head,
    expectedBase = state.base,
    expectedVersion = state.version,
  }
  local fingerprint = vim.json.encode(input)
  if draft.fingerprint ~= fingerprint then
    draft.requestKey, draft.fingerprint = store.request_key(), fingerprint
  end
  input.requestKey = draft.requestKey
  rpc.call("SubmitReview", input, function(err, result)
    Private.apply_result(state, err, result, callback)
  end)
end

---@param state lgtm.ReviewState
function M.get_reconcile_error(state)
  return store.get_readiness_error(state) or Private.get_recovery_error(state)
end

---@param state lgtm.ReviewState
function M.reconcile(state, callback)
  local err = M.get_reconcile_error(state)
  if err then
    callback(err, { message = err.message })
    return
  end
  rpc.call("ReconcileSubmission", { reviewId = state.review.id }, function(error, result)
    Private.apply_result(state, error, result, callback)
  end)
end

---@param state lgtm.ReviewState
function M.recover(state, callback)
  local err = Private.get_recovery_error(state)
  if err then
    callback(err, { message = err.message })
    return
  end
  rpc.call("ReconcileSubmission", { reviewId = state.review.id }, function(error, result)
    if error or result.review.submissionStatus ~= "unsubmitted" then
      Private.apply_result(state, error, result, callback)
      return
    end
    store.apply(state, result.review)
    local draft = store.submissions[state.review.id]
    if draft then
      draft.uncertain, draft.fingerprint = false, nil
    end
    M.refresh(state, function(refresh_error)
      if refresh_error then
        callback(refresh_error, { message = refresh_error.message })
        return
      end
      callback(nil, { message = "Drafts recovered; choose a review outcome", editable = true })
    end)
  end)
end

function M.get_options()
  return { "working", "staged", "branch", "PR" }
end

function M.create_local(root, mode, base_branch, callback, options)
  require("lgtm.services.forge_service").get_comparison(
    root,
    mode,
    base_branch,
    function(err, checkout, comparison)
      if err then
        callback(err)
        return
      end
      M.create(
        { localGit = { path = checkout.root, branch = checkout.branch } },
        comparison,
        checkout.root,
        callback,
        options
      )
    end
  )
end

---@return lgtm.ReviewState
function M.create_pr(repository_url, pr, callback)
  local state = {
    target = { pullRequest = { repositoryUrl = repository_url, number = pr.number } },
    comparison = { pullRequest = { baseCommit = pr.base, headCommit = pr.head } },
    base = pr.base,
    head = pr.head,
    mode = "pr",
    kind = "pr",
    initializing = true,
  }
  local checkout
  local request_key = store.request_key()
  local function start()
    M.set_status(state, true)
    M.create(state.target, state.comparison, nil, function(err, loaded)
      if state.closed then
        return
      end
      M.set_status(state, false, err)
      if err then
        callback(err, state)
        return
      end
      local base, head = state.base, state.head
      store.apply(state, loaded.review)
      state.resume_notice = loaded.resume_notice
      if store.current == state then
        callback(nil, state, base ~= state.base or head ~= state.head)
      end
    end, { localGitPath = checkout, requestKey = request_key })
  end
  state.retry_load = function()
    if not state.initializing then
      start()
    end
  end
  require("lgtm.services.forge_service").get_repository_root(vim.fn.getcwd(), function(_, root)
    if state.closed then
      return
    end
    checkout = root
    start()
  end)
  return state
end

-- Private ---------------------------------------------------------------------

function Private.apply_result(state, err, result, done)
  if err then
    local draft = store.submissions[state.review.id]
    if draft then
      draft.uncertain = err.uncertain
    end
    local message = (
      err.uncertain and "Submission response lost. Use :Lgtm reconcile before retrying. "
      or "Submission failed. "
    ) .. (err.message or "Unknown error")
    M.refresh(state, function()
      done(err, { message = message })
    end)
    return
  end
  store.apply(state, result.review)
  local status = result.review.submissionStatus
  if status == "confirmed" then
    store.submissions[state.review.id] = nil
    require("lgtm.services.threads_service").get_conversation(state, function(read_error)
      done(nil, {
        message = "Review submitted",
        warning = read_error
            and ("Review published; reload comments with :Lgtm refresh. " .. (read_error.message or ""))
          or nil,
      })
    end)
  elseif status == "unsubmitted" then
    local draft = store.submissions[state.review.id]
    if draft then
      draft.uncertain, draft.fingerprint = false, nil
    end
    local error = {
      message = "Review is editable; drafts and final text are saved. Choose an outcome with :Lgtm submit. "
        .. (result.review.submissionError or ""),
    }
    M.refresh(state, function()
      done(error, { message = error.message })
    end)
  else
    local pending =
      vim.list_contains({ "uncertain", "pending", "posting", "sending", "reconciling" }, status)
    local error = {
      message = pending and ("Submission " .. status .. "; use :Lgtm reconcile before retrying")
        or (
          (result.review.submissionError or ("Submission " .. tostring(status)))
          .. ". Use :Lgtm reconcile to inspect/recover."
        ),
    }
    done(error, { message = error.message })
  end
end

function Private.get_recovery_error(state)
  if
    state.review.submissionStatus == "failed"
    and not vim.list_contains(rpc.capabilities or {}, "rejected_submission_recovery")
  then
    return {
      message = "Update the LGTM CLI/backend before recovering this rejected submission. Your drafts are preserved.",
    }
  end
end

function Private.catalog_status(loading, err)
  catalog = catalog
    or require("nui-components.signal").create({ value = { reviews = {}, pull_requests = {} } })
  catalog.value =
    vim.tbl_extend("force", catalog:get_value().value, { loading = loading, error = err or false })
end
return M
