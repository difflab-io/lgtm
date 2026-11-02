local n = require("nui-components")
local PullRequest = require("lgtm.models.pull_request")
local ReviewSubmission = require("lgtm.models.review_submission")
local Thread = require("lgtm.models.thread")

local PrService = {}
PrService.__index = PrService

local Private = {}

-- State -----------------------------------------------------------------------

---Construct a reactive PrService for branch PR discovery, PR-scoped threads, and review submissions.
---@param opts { seed_prs?: table[], pr_fetcher?: fun(branch: string|nil): table[] }|nil
---@return table
function PrService.new(opts)
  opts = opts or {}
  local seed_prs = Private.normalize_prs(opts.seed_prs or {})

  local self = setmetatable({
    pr_fetcher = opts.pr_fetcher,
    seed_prs = seed_prs,
    threads_by_pr = {},
    reviewed_by_pr = {},
    submissions_by_pr = {},
    state = n.create_signal({
      open_prs = seed_prs,
      selected_pr = nil,
      active_tabpage = nil,
      threads = {},
      selected_thread_id = nil,
      active_view = "summary",
      reviewed_files = {},
      submissions = {},
      is_loading = false,
      error = nil,
    }),
  }, PrService)

  return self
end

-- Methods ---------------------------------------------------------------------

---Replace the configured seed PRs and refresh `state.open_prs`.
---@param raw_prs table[]
---@return LgtmPullRequest[]
function PrService:set_seed_prs(raw_prs)
  self.seed_prs = Private.normalize_prs(raw_prs or {})
  self.state.open_prs = vim.list_slice(self.seed_prs)
  return self.state:get_value().open_prs
end

---Reset all PR sessions, threads, and submissions.
function PrService:reset()
  self.threads_by_pr = {}
  self.reviewed_by_pr = {}
  self.submissions_by_pr = {}
  self.state.open_prs = vim.list_slice(self.seed_prs)
  self.state.selected_pr = nil
  self.state.active_tabpage = nil
  self.state.threads = {}
  self.state.selected_thread_id = nil
  self.state.active_view = "summary"
  self.state.reviewed_files = {}
  self.state.submissions = {}
  self.state.is_loading = false
  self.state.error = nil
end

---Discover open PRs for the given branch (or current Git branch) via seed_prs, custom fetcher, or `gh pr list`.
---@param branch string|nil
---@param callback fun(prs: LgtmPullRequest[], err: string|nil)|nil
---@return LgtmPullRequest[], string|nil
function PrService:get_open_prs(branch, callback)
  self.state.is_loading = true
  self.state.error = nil

  local resolved_branch = type(branch) == "string" and vim.trim(branch) or ""
  if resolved_branch == "" then
    resolved_branch = Private.current_git_branch()
  end

  local ok, result = pcall(function()
    if self.pr_fetcher then
      return Private.normalize_prs(self.pr_fetcher(resolved_branch) or {})
    end
    if #self.seed_prs > 0 then
      return Private.filter_prs_by_branch(self.seed_prs, resolved_branch)
    end
    return Private.fetch_prs_via_gh(resolved_branch)
  end)

  self.state.is_loading = false
  if not ok then
    local err = tostring(result)
    self.state.error = err
    if callback then
      callback({}, err)
    end
    return {}, err
  end

  self.state.open_prs = result
  if callback then
    callback(result, nil)
  end
  return result, nil
end

---Select an open PR by number, activating PR review state for that PR.
---@param pr_number integer
---@param callback fun(pr: LgtmPullRequest|nil, err: string|nil)|nil
---@return LgtmPullRequest
function PrService:select_pr(pr_number, callback)
  local target_number = tonumber(pr_number)
  local snapshot = self.state:get_value()
  local matched = nil
  for _, pr in ipairs(snapshot.open_prs) do
    if pr.number == target_number then
      matched = pr
      break
    end
  end
  if not matched then
    for _, pr in ipairs(self.seed_prs) do
      if pr.number == target_number then
        matched = pr
        break
      end
    end
  end
  assert(matched ~= nil, "pull request not found: #" .. tostring(pr_number))

  local pr_threads = vim.list_slice(self.threads_by_pr[matched.number] or {})
  local pr_reviewed = vim.tbl_extend("force", {}, self.reviewed_by_pr[matched.number] or {})
  local pr_submissions = vim.list_slice(self.submissions_by_pr[matched.number] or {})

  self.state.selected_pr = matched
  self.state.threads = pr_threads
  self.state.selected_thread_id = pr_threads[1] and pr_threads[1].id or nil
  self.state.active_view = "summary"
  self.state.reviewed_files = pr_reviewed
  self.state.submissions = pr_submissions
  self.state.error = nil

  if callback then
    callback(matched, nil)
  end
  return matched
end

---Bind the active `CodeDiff` tabpage handle for the selected PR session.
---@param tabpage integer|nil
function PrService:set_active_tabpage(tabpage)
  self.state.active_tabpage = tabpage
end

---Return true when the given tabpage belongs to the active PR review session.
---@param tabpage integer|nil
---@return boolean
function PrService:is_pr_tabpage(tabpage)
  local snapshot = self.state:get_value()
  if not snapshot.selected_pr then
    return false
  end
  if snapshot.active_tabpage == nil then
    return true
  end
  return snapshot.active_tabpage == (tabpage or vim.api.nvim_get_current_tabpage())
end

---Close the current PR review session while preserving drafted PR threads in memory.
function PrService:close_pr_session()
  self.state.selected_pr = nil
  self.state.active_tabpage = nil
  self.state.threads = {}
  self.state.selected_thread_id = nil
  self.state.active_view = "summary"
end

---Return all threads for the currently selected PR.
---@param opts { path?: string, side?: "LEFT"|"RIGHT", include_resolved?: boolean }|nil
---@return LgtmThread[]
function PrService:get_threads(opts)
  opts = opts or {}
  local snapshot = self.state:get_value()
  local normalized_path = opts.path and Private.normalize_path(opts.path) or nil
  local include_resolved = opts.include_resolved ~= false
  local list = {}
  for _, thread in ipairs(snapshot.threads) do
    local path_matches = not normalized_path or thread.path == normalized_path
    local side_matches = not opts.side or thread.side == opts.side
    local resolved_matches = include_resolved or not thread.resolved
    if path_matches and side_matches and resolved_matches then
      table.insert(list, thread)
    end
  end
  return list
end

---Return all PR threads for a file path (and optional diff side), sorted by line.
---@param path string
---@param side "LEFT"|"RIGHT"|nil
---@return LgtmThread[]
function PrService:get_threads_for_file(path, side)
  local list = self:get_threads({ path = path, side = side, include_resolved = true })
  table.sort(list, function(a, b)
    if a.line == b.line then
      return a.id < b.id
    end
    return a.line < b.line
  end)
  return list
end

---Return a single PR thread by ID.
---@param thread_id string|nil
---@return LgtmThread|nil
function PrService:get_thread(thread_id)
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

---Return the first PR thread anchored at or covering `line` in `path`.
---@param path string
---@param line integer
---@param side "LEFT"|"RIGHT"|nil
---@return LgtmThread|nil
function PrService:get_thread_at_line(path, line, side)
  local threads = self:get_threads_for_file(path, side)
  for _, thread in ipairs(threads) do
    if line >= thread.start_line and line <= thread.line then
      return thread
    end
  end
  return nil
end

---Create a new PR review thread on the active PR.
---@param input { path: string, side?: "LEFT"|"RIGHT", start_line?: integer, line: integer, body: string, author?: string }
---@return LgtmThread
function PrService:create_thread(input)
  local selected_pr = assert(self.state:get_value().selected_pr, "no active pull request selected")
  local payload = vim.tbl_extend("force", {}, input or {}, {
    mode = "pr",
    pr_number = selected_pr.number,
    side = input and input.side or "RIGHT",
    state = "PENDING",
  })
  local created = Thread.new(payload)
  local next_threads = vim.list_slice(self.state:get_value().threads or {})
  table.insert(next_threads, created)
  Private.sort_threads(next_threads)
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  self.state.selected_thread_id = created.id
  self.state.error = nil
  return created
end

---Reply to an existing PR thread.
---@param thread_id string
---@param body string
---@param author string|nil
---@return LgtmThread, LgtmComment
function PrService:reply_to_thread(thread_id, body, author)
  local selected_pr = assert(self.state:get_value().selected_pr, "no active pull request selected")
  local updated_thread = nil
  local created_comment = nil
  local next_threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread, created_comment = Thread.with_reply(thread, {
        body = body,
        author = author,
        state = "PENDING",
      })
      return updated_thread
    end
  )
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  self.state.selected_thread_id = thread_id
  return updated_thread, created_comment
end

---Edit an existing comment in a PR thread.
---@param thread_id string
---@param comment_id string
---@param body string
---@return LgtmThread, LgtmComment
function PrService:edit_comment(thread_id, comment_id, body)
  local selected_pr = assert(self.state:get_value().selected_pr, "no active pull request selected")
  local updated_thread = nil
  local updated_comment = nil
  local next_threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread, updated_comment = Thread.with_edited_comment(thread, comment_id, body)
      return updated_thread
    end
  )
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  return updated_thread, updated_comment
end

---Delete a comment from a PR thread, removing the thread if empty.
---@param thread_id string
---@param comment_id string
---@return LgtmThread|nil
function PrService:delete_comment(thread_id, comment_id)
  local snapshot = self.state:get_value()
  local selected_pr = assert(snapshot.selected_pr, "no active pull request selected")
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
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  if not resulting_thread and snapshot.selected_thread_id == thread_id then
    self.state.selected_thread_id = next_threads[1] and next_threads[1].id or nil
    self.state.active_view = "summary"
  end
  return resulting_thread
end

---Toggle resolved status of a PR thread.
---@param thread_id string
---@return LgtmThread
function PrService:toggle_resolved(thread_id)
  local selected_pr = assert(self.state:get_value().selected_pr, "no active pull request selected")
  local updated_thread = nil
  local next_threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread = Thread.with_resolved(thread, not thread.resolved)
      return updated_thread
    end
  )
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  return updated_thread
end

---Toggle collapsed status of a PR thread.
---@param thread_id string
---@return LgtmThread
function PrService:toggle_collapsed(thread_id)
  local selected_pr = assert(self.state:get_value().selected_pr, "no active pull request selected")
  local updated_thread = nil
  local next_threads = Private.map_thread(
    self.state:get_value().threads,
    thread_id,
    function(thread)
      updated_thread = Thread.with_collapsed(thread, not thread.collapsed)
      return updated_thread
    end
  )
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
  return updated_thread
end

---Collapse or uncollapse all threads on the active PR.
---@param collapsed boolean
function PrService:set_all_collapsed(collapsed)
  local snapshot = self.state:get_value()
  local selected_pr = assert(snapshot.selected_pr, "no active pull request selected")
  local next_threads = {}
  for _, thread in ipairs(snapshot.threads) do
    table.insert(next_threads, Thread.with_collapsed(thread, collapsed))
  end
  self.threads_by_pr[selected_pr.number] = next_threads
  self.state.threads = next_threads
end

---Toggle whether a file path is marked as reviewed on the active PR.
---@param path string
---@return boolean
function PrService:toggle_file_reviewed(path)
  local snapshot = self.state:get_value()
  local selected_pr = assert(snapshot.selected_pr, "no active pull request selected")
  local normalized = Private.normalize_path(path)
  assert(normalized ~= "", "file path is required")
  local next_reviewed = vim.tbl_extend("force", {}, snapshot.reviewed_files)
  local is_reviewed = not next_reviewed[normalized]
  if is_reviewed then
    next_reviewed[normalized] = true
  else
    next_reviewed[normalized] = nil
  end
  self.reviewed_by_pr[selected_pr.number] = next_reviewed
  self.state.reviewed_files = next_reviewed
  return is_reviewed
end

---Select a PR thread ID.
---@param thread_id string|nil
function PrService:select_thread(thread_id)
  if thread_id ~= nil then
    assert(self:get_thread(thread_id) ~= nil, "thread not found: " .. tostring(thread_id))
  end
  self.state.selected_thread_id = thread_id
end

---Select a PR thread and switch the threads pane state into `detail` view.
---@param thread_id string
---@return LgtmThread
function PrService:open_thread_detail(thread_id)
  local thread = assert(self:get_thread(thread_id), "thread not found: " .. tostring(thread_id))
  self.state.selected_thread_id = thread.id
  self.state.active_view = "detail"
  return thread
end

---Return the threads pane state from `detail` view back to `summary` view.
function PrService:close_thread_detail()
  self.state.active_view = "summary"
end

---Submit an overall PR review (`comment`, `approve`, or `request_changes`) and mark pending comments published.
---@param input { outcome: "comment"|"approve"|"request_changes", body: string }
---@param callback fun(submission: LgtmReviewSubmission|nil, err: string|nil)|nil
---@return LgtmReviewSubmission
function PrService:submit_review(input, callback)
  local snapshot = self.state:get_value()
  local selected_pr = assert(snapshot.selected_pr, "no active pull request selected")
  local submission = ReviewSubmission.new({
    pr_number = selected_pr.number,
    outcome = input and input.outcome or "comment",
    body = input and input.body or "",
  })

  local next_submissions = vim.list_slice(snapshot.submissions or {})
  table.insert(next_submissions, submission)
  self.submissions_by_pr[selected_pr.number] = next_submissions
  self.state.submissions = next_submissions

  local published_threads = {}
  for _, thread in ipairs(snapshot.threads) do
    local comments = {}
    for _, comment in ipairs(thread.comments) do
      table.insert(
        comments,
        vim.tbl_extend("force", {}, comment, {
          state = "PUBLISHED",
        })
      )
    end
    table.insert(
      published_threads,
      vim.tbl_extend("force", {}, thread, {
        comments = comments,
      })
    )
  end
  self.threads_by_pr[selected_pr.number] = published_threads
  self.state.threads = published_threads
  self.state.error = nil

  if callback then
    callback(submission, nil)
  end
  return submission
end

-- Private ---------------------------------------------------------------------

function Private.normalize_path(path)
  if type(path) ~= "string" then
    return ""
  end
  return vim.trim(path):gsub("^%./", "")
end

function Private.normalize_prs(raw_prs)
  local list = {}
  for _, raw in ipairs(raw_prs or {}) do
    table.insert(list, PullRequest.new(raw))
  end
  return list
end

function Private.filter_prs_by_branch(prs, branch)
  if branch == "" then
    return vim.list_slice(prs or {})
  end
  local matched = {}
  for _, pr in ipairs(prs) do
    if pr.source_branch == branch or pr.target_branch == branch then
      table.insert(matched, pr)
    end
  end
  if #matched > 0 then
    return matched
  end
  return vim.list_slice(prs or {})
end

function Private.current_git_branch()
  local output = vim.fn.systemlist({ "git", "rev-parse", "--abbrev-ref", "HEAD" })
  if vim.v.shell_error ~= 0 or not output[1] then
    return ""
  end
  return vim.trim(output[1])
end

function Private.fetch_prs_via_gh(branch)
  local fields = table.concat({
    "number",
    "title",
    "body",
    "author",
    "headRefName",
    "baseRefName",
    "state",
    "isDraft",
    "baseRefOid",
    "headRefOid",
    "updatedAt",
    "additions",
    "deletions",
    "files",
  }, ",")
  local cmd = { "gh", "pr", "list", "--state", "open", "--json", fields }
  if branch ~= "" and branch ~= "HEAD" then
    table.insert(cmd, "--head")
    table.insert(cmd, branch)
  end
  local raw = vim.fn.system(cmd)
  if vim.v.shell_error ~= 0 then
    return {}
  end
  local ok, decoded = pcall(vim.json.decode, raw)
  if not ok or type(decoded) ~= "table" then
    return {}
  end
  return Private.normalize_prs(decoded)
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

return PrService
