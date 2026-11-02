---@class LgtmPullRequestFile
---@field path string
---@field old_path string|nil
---@field status "A"|"M"|"D"|"R"

---@class LgtmPullRequest
---@field id string
---@field number integer
---@field title string
---@field description string
---@field author string
---@field source_branch string
---@field target_branch string
---@field state "open"|"draft"
---@field base_revision string
---@field head_revision string
---@field updated_at string
---@field lines_added integer
---@field lines_removed integer
---@field files LgtmPullRequestFile[]

local PullRequest = {}
local Private = {}

-- Methods ---------------------------------------------------------------------

---Construct and validate a PullRequest model from normalized or `gh pr list` JSON fields.
---@param input table
---@return LgtmPullRequest
function PullRequest.new(input)
  assert(type(input) == "table", "pull request input must be a table")
  local number = tonumber(input.number)
  assert(number and number >= 1, "pull request number must be >= 1")
  local title = type(input.title) == "string" and vim.trim(input.title) or ""
  assert(title ~= "", "pull request title is required")

  local raw_source = input.source_branch or input.headRefName
  local source_branch = type(raw_source) == "string" and vim.trim(raw_source) or ""
  assert(source_branch ~= "", "pull request source_branch is required")
  local raw_target = input.target_branch or input.baseRefName or "main"
  local target_branch = type(raw_target) == "string" and vim.trim(raw_target) or "main"
  local author = Private.extract_author(input.author)
  local state = Private.normalize_state(input.state, input.isDraft)
  local raw_base = input.base_revision or input.baseRefOid or target_branch
  local raw_head = input.head_revision or input.headRefOid or source_branch
  local raw_updated = input.updated_at or input.updatedAt or "just now"

  return {
    id = tostring(input.id or string.format("pr-%d", number)),
    number = number,
    title = title,
    description = Private.normalize_description(input.description or input.body),
    author = author,
    source_branch = source_branch,
    target_branch = target_branch,
    state = state,
    base_revision = type(raw_base) == "string" and vim.trim(raw_base) or target_branch,
    head_revision = type(raw_head) == "string" and vim.trim(raw_head) or source_branch,
    updated_at = type(raw_updated) == "string" and vim.trim(raw_updated) or "just now",
    lines_added = tonumber(input.lines_added or input.additions) or 0,
    lines_removed = tonumber(input.lines_removed or input.deletions) or 0,
    files = Private.normalize_files(input.files),
  }
end

---Return true when the pull request matches a search query across number, title, author, or branch.
---@param pr LgtmPullRequest
---@param query string|nil
---@return boolean
function PullRequest.matches_query(pr, query)
  local needle = (type(query) == "string" and vim.trim(query) or ""):lower()
  if needle == "" then
    return true
  end
  local haystack = string
    .format("#%d %s %s %s %s", pr.number, pr.title, pr.author, pr.source_branch, pr.target_branch)
    :lower()
  return haystack:find(needle, 1, true) ~= nil
end

---Format a single-line summary for the left pane of `pr-select-modal`.
---@param pr LgtmPullRequest
---@return string
function PullRequest.format_list_label(pr)
  local badge = pr.state == "draft" and "[DRAFT] " or ""
  return string.format("#%d %s%s (@%s)", pr.number, badge, pr.title, pr.author)
end

---Format Markdown preview lines for the right pane of `pr-select-modal`.
---@param pr LgtmPullRequest|nil
---@return string[]
function PullRequest.format_preview_lines(pr)
  if not pr then
    return { "No pull request selected." }
  end
  local lines = {
    string.format("# #%d %s", pr.number, pr.title),
    "",
    string.format("- **Author:** @%s", pr.author),
    string.format("- **Branches:** `%s` → `%s`", pr.source_branch, pr.target_branch),
    string.format("- **Status:** %s", pr.state),
    string.format("- **Changes:** +%d / -%d", pr.lines_added, pr.lines_removed),
    "",
    "## Description",
    "",
  }
  local description = pr.description ~= "" and pr.description or "_No description provided._"
  for _, line in ipairs(vim.split(description, "\n", { plain = true })) do
    table.insert(lines, line)
  end
  if #pr.files > 0 then
    table.insert(lines, "")
    table.insert(lines, "## Changed Files")
    table.insert(lines, "")
    for _, file in ipairs(pr.files) do
      table.insert(lines, string.format("- [%s] `%s`", file.status, file.path))
    end
  end
  return lines
end

-- Private ---------------------------------------------------------------------

function Private.extract_author(author)
  if type(author) == "table" then
    local raw = author.login or author.name or "unknown"
    return type(raw) == "string" and vim.trim(raw) or "unknown"
  end
  local trimmed = type(author) == "string" and vim.trim(author) or ""
  return trimmed ~= "" and trimmed or "unknown"
end

function Private.normalize_state(state, is_draft)
  if is_draft == true then
    return "draft"
  end
  local normalized = (type(state) == "string" and vim.trim(state) or ""):lower()
  if normalized == "draft" then
    return "draft"
  end
  return "open"
end

function Private.normalize_description(text)
  if type(text) ~= "string" then
    return ""
  end
  return (text:gsub("\r\n", "\n"):gsub("%s+$", ""))
end

function Private.normalize_files(files)
  local normalized = {}
  if type(files) ~= "table" then
    return normalized
  end
  for _, entry in ipairs(files) do
    if type(entry) == "string" then
      table.insert(normalized, { path = entry, status = "M" })
    elseif
      type(entry) == "table"
      and type(entry.path) == "string"
      and vim.trim(entry.path) ~= ""
    then
      local raw_status = type(entry.status) == "string" and vim.trim(entry.status) or "M"
      local status = raw_status:upper():sub(1, 1)
      if status ~= "A" and status ~= "M" and status ~= "D" and status ~= "R" then
        status = "M"
      end
      table.insert(normalized, {
        path = vim.trim(entry.path),
        old_path = type(entry.old_path) == "string" and vim.trim(entry.old_path) or nil,
        status = status,
      })
    end
  end
  return normalized
end

return PullRequest
