-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local rpc = require("lgtm.services.client")
local store = require("lgtm.services.state")

-- Public ----------------------------------------------------------------------

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
    state.refresh(function(refresh_error)
      if refresh_error then
        callback(refresh_error, { message = refresh_error.message })
        return
      end
      callback(nil, { message = "Drafts recovered; choose a review outcome", editable = true })
    end)
  end)
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
    state.refresh(function()
      done(err, { message = message })
    end)
    return
  end
  store.apply(state, result.review)
  local status = result.review.submissionStatus
  if status == "confirmed" then
    store.submissions[state.review.id] = nil
    require("lgtm.services.reviews").get_conversation(state, function(read_error)
      if not read_error and state.render then
        state.render()
      end
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
    state.refresh(function()
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

return M
