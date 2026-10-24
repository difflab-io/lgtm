---@class lgtm.ServiceError
---@field message string
---@field code string?
---@field uncertain boolean?

---@class lgtm.Review
---@field id string
---@field version string
---@field state 'active'|'completed'
---@field target table Backend target union, retained without remapping.
---@field comparison table Backend comparison union, retained without remapping.
---@field submissionStatus string
---@field body string?

---@class lgtm.Thread
---@field id string
---@field version string
---@field state string
---@field anchor table Persisted source and location evidence.
---@field currentPath string?
---@field orphanReason string?

---@class lgtm.Comment
---@field id string
---@field threadId string
---@field ownerId string
---@field author table
---@field body string
---@field version string
---@field position string|number
---@field visibility string
---@field originSessionId string?

---@class lgtm.ReviewFile
---@field path string
---@field identity string?
---@field content string?
---@field baseContent string?
---@field contentHash string?
---@field loaded boolean?

---@class lgtm.Conversation
---@field version string
---@field threads lgtm.Thread[]
---@field comments lgtm.Comment[]

---@class lgtm.ReviewState
---@field review lgtm.Review?
---@field scope table Backend review or saved-file scope.
---@field version string?
---@field files lgtm.ReviewFile[]?
---@field file lgtm.ReviewFile?
---@field conversation lgtm.Conversation?
---@field reviewed table<string, string>?
---@field kind 'git'|'directory'|'pr'|'notes'
---@field closed boolean?
---@field initializing boolean?
---@field on_error fun(err: lgtm.ServiceError)?
---@field on_files_changed fun()?
---@field refresh fun(done: function?) Refresh the owning editor view.

---@class lgtm.Draft
---@field body string?
---@field requestKey string Stable across retries of the same write.
---@field expectedVersion string?
---@field scope table?
---@field anchor table?
---@field outcome string?
---@field fingerprint string?
---@field uncertain boolean?

-- State -----------------------------------------------------------------------
local M = {
  current = nil,
  generation = 0,
  tabs = {},
  notes = {},
  submissions = {},
}

-- Public ----------------------------------------------------------------------
---@param state lgtm.ReviewState
function M.set(state)
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
    require("lgtm.services.files").get_reviewed(state)
  end
  return state
end

function M.create_notes(path)
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
