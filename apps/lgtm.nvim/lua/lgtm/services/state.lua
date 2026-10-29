-- State -----------------------------------------------------------------------

local Private = {}

local M = {
  current = nil,
  generation = 0,
  tabs = {},
  submissions = {},
}

-- Methods ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.set(state)
  if state then
    require("lgtm.services.threads_service").get_data(state)
  end
  if M.current ~= state then
    M.generation = M.generation + 1
    M.current = state
  end
end

function M.request_key()
  return vim.fn.sha256(tostring(vim.uv.hrtime()) .. tostring(math.random()))
end

---@param state lgtm.ReviewState
function M.get_readiness_error(state)
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

---@param state lgtm.ReviewState
function M.apply(state, review)
  require("lgtm.services.threads_service").get_data(state)
  if
    state.version
    and tonumber(review.version)
    and tonumber(state.version)
    and tonumber(review.version) < tonumber(state.version)
  then
    return state
  end
  local snapshot = Private.selected_snapshot(state, review)
  local snapshot_changed = state.snapshot
    and snapshot
    and (
      state.snapshot.root ~= snapshot.root
      or state.snapshot.baseCommit ~= snapshot.baseCommit
      or state.snapshot.headCommit ~= snapshot.headCommit
    )
  local comparison_changed = not vim.deep_equal(state.comparison, review.comparison)
  local load_progress = state.reviewed_review ~= review.id or comparison_changed or snapshot_changed
  if snapshot_changed or state.kind == "pr" and state.comparison and comparison_changed then
    state.views = {}
    require("lgtm.services.files_service").set_reviewed(state, {})
    state.source_generation = (state.source_generation or 0) + 1
    state.render_ready = false
  end
  if review.target.directory and state.root ~= review.target.directory.path then
    state.render_ready = false
    state.views = {}
    state.source_generation = (state.source_generation or 0) + 1
  end
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
  if snapshot then
    state.snapshot = snapshot
    state.git_root = vim.uv.fs_stat(snapshot.root) and snapshot.root or nil
    state.base = snapshot.baseCommit
    state.head = snapshot.headCommit
    state.git_checked = true
  end
  state.scope_key = review.id
  require("lgtm.services.session_service").set_review(state, review)
  if load_progress then
    require("lgtm.services.files_service").get_reviewed(state)
  end
  return state
end

-- Private ---------------------------------------------------------------------

function Private.selected_snapshot(state, review)
  for _, revision in ipairs(review.revisions or {}) do
    if revision.id == state.selected_revision_id then
      return revision
    end
  end
  return review.snapshot
end
return M
