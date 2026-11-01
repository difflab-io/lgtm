local ReviewSubmission = require("lgtm.models.review_submission")
local Thread = require("lgtm.models.thread")

local Notifications = {}
local Private = {}

-- State -----------------------------------------------------------------------

local history = {}

-- Methods ---------------------------------------------------------------------

---Emit a notification through `nvim-notify` and record it in history.
---@param message string
---@param level integer|string|nil
---@param opts table|nil
---@return table
function Notifications.notify(message, level, opts)
  opts = opts or {}
  local resolved_level = Private.resolve_level(level)
  local title = opts.title or "LGTM"
  local entry = {
    message = tostring(message),
    level = resolved_level,
    title = title,
    event = opts.event or "general",
    timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ"),
  }
  table.insert(history, entry)
  Private.dispatch_to_notify(entry.message, resolved_level, title)
  return entry
end

---Notify when a pull request review session is opened.
---@param pr LgtmPullRequest
---@return table
function Notifications.pr_selected(pr)
  return Notifications.notify(
    string.format("Opened PR #%d for review: %s", pr.number, pr.title),
    vim.log.levels.INFO,
    { title = "LGTM · PR Review", event = "pr_selected" }
  )
end

---Notify when a new thread is created in local or PR mode.
---@param thread LgtmThread
---@return table
function Notifications.thread_created(thread)
  local anchor = Thread.format_anchor(thread)
  local label = thread.mode == "pr" and "Drafted PR comment" or "Opened local thread"
  return Notifications.notify(
    string.format("%s at %s", label, anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Thread", event = "thread_created" }
  )
end

---Notify when a reply is added to a thread.
---@param thread LgtmThread
---@return table
function Notifications.reply_added(thread)
  local anchor = Thread.format_anchor(thread)
  return Notifications.notify(
    string.format("Added reply at %s", anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Reply", event = "reply_added" }
  )
end

---Notify when a comment is edited.
---@param thread LgtmThread
---@return table
function Notifications.comment_updated(thread)
  local anchor = Thread.format_anchor(thread)
  return Notifications.notify(
    string.format("Updated comment at %s", anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Comment", event = "comment_updated" }
  )
end

---Notify when a comment is deleted.
---@param thread LgtmThread
---@return table
function Notifications.comment_deleted(thread)
  local anchor = Thread.format_anchor(thread)
  return Notifications.notify(
    string.format("Deleted comment at %s", anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Comment", event = "comment_deleted" }
  )
end

---Notify when a thread's resolved status changes.
---@param thread LgtmThread
---@return table
function Notifications.thread_resolved(thread)
  local anchor = Thread.format_anchor(thread)
  local action = thread.resolved and "Resolved thread" or "Reopened thread"
  return Notifications.notify(
    string.format("%s at %s", action, anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Thread", event = "thread_resolved" }
  )
end

---Notify when a thread is opened in the threads-pane detail view.
---@param thread LgtmThread
---@return table
function Notifications.thread_opened(thread)
  local anchor = Thread.format_anchor(thread)
  return Notifications.notify(
    string.format("Viewing thread at %s", anchor),
    vim.log.levels.INFO,
    { title = "LGTM · Thread Detail", event = "thread_opened" }
  )
end

---Notify when an overall PR review is submitted.
---@param pr LgtmPullRequest
---@param submission LgtmReviewSubmission
---@return table
function Notifications.review_submitted(pr, submission)
  local label = ReviewSubmission.outcome_label(submission.outcome)
  return Notifications.notify(
    string.format("Submitted %s review on PR #%d", label, pr.number),
    vim.log.levels.INFO,
    { title = "LGTM · Review Submitted", event = "review_submitted" }
  )
end

---Return a shallow copy of all recorded notifications.
---@return table[]
function Notifications.get_history()
  local copy = {}
  for _, item in ipairs(history) do
    table.insert(copy, vim.tbl_extend("force", {}, item))
  end
  return copy
end

---Return the most recent notification entry, or nil if none.
---@return table|nil
function Notifications.last()
  return history[#history]
end

---Clear recorded notification history.
function Notifications.clear()
  history = {}
end

-- Private ---------------------------------------------------------------------

function Private.resolve_level(level)
  if type(level) == "number" then
    return level
  end
  if level == "warn" or level == "WARN" then
    return vim.log.levels.WARN
  end
  if level == "error" or level == "ERROR" then
    return vim.log.levels.ERROR
  end
  return vim.log.levels.INFO
end

function Private.dispatch_to_notify(message, level, title)
  local ok, notify = pcall(require, "notify")
  if ok and type(notify) == "function" then
    pcall(notify, message, level, {
      title = title,
      timeout = 1500,
    })
  end
end

return Notifications
