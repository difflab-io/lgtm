---@class LgtmReviewSubmission
---@field id string
---@field pr_number integer
---@field outcome "comment"|"approve"|"request_changes"
---@field body string
---@field submitted_at string

local ReviewSubmission = {}
local Private = {}

-- Methods ---------------------------------------------------------------------

---Validate and construct an overall PR review submission entity.
---@param input table
---@return LgtmReviewSubmission
function ReviewSubmission.new(input)
  assert(type(input) == "table", "review submission input must be a table")
  local pr_number = tonumber(input.pr_number)
  assert(pr_number and pr_number >= 1, "review submission pr_number must be >= 1")
  local outcome = Private.normalize_outcome(input.outcome)
  local body = type(input.body) == "string" and vim.trim(input.body) or ""
  if outcome == "comment" or outcome == "request_changes" then
    assert(body ~= "", string.format("review body is required for %s submissions", outcome))
  end

  return {
    id = tostring(input.id or Private.next_id()),
    pr_number = pr_number,
    outcome = outcome,
    body = body,
    submitted_at = input.submitted_at or os.date("!%Y-%m-%dT%H:%M:%SZ"),
  }
end

---Return human-readable label for a review outcome.
---@param outcome string
---@return string
function ReviewSubmission.outcome_label(outcome)
  local normalized = Private.normalize_outcome(outcome)
  if normalized == "approve" then
    return "Approve"
  elseif normalized == "request_changes" then
    return "Request Changes"
  end
  return "Comment"
end

-- Private ---------------------------------------------------------------------

local sequence = 0

function Private.next_id()
  sequence = sequence + 1
  return string.format("review-%d", sequence)
end

function Private.normalize_outcome(outcome)
  local value = (type(outcome) == "string" and vim.trim(outcome) or ""):lower()
  if value == "" then
    value = "comment"
  end
  assert(
    value == "comment" or value == "approve" or value == "request_changes",
    "invalid review outcome: " .. tostring(outcome)
  )
  return value
end

return ReviewSubmission
