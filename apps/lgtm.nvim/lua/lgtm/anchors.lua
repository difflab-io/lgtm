-- Source-bound placement. CodeDiff supplies buffers and rename candidates, not persistent anchors.
local M = {}

-- Match Rust str::lines(): CRLF is one ending; final newlines are not extra lines.
function M.lines(bytes)
  local lines, crlf, start = {}, {}, 1
  while true do
    local ending = bytes:find("\n", start, true)
    if not ending then
      if start <= #bytes then
        lines[#lines + 1] = bytes:sub(start)
      end
      return lines, crlf
    end
    local line = bytes:sub(start, ending - 1)
    if line:sub(-1) == "\r" then
      line = line:sub(1, -2)
      crlf[#lines + 1] = true
    end
    lines[#lines + 1] = line
    start = ending + 1
  end
end

local function valid_source(source)
  if not source or type(source.content_hash) ~= "string" then
    return false
  end
  if #source.content_hash ~= 71 or not source.content_hash:match("^sha256:%x+$") then
    return false
  end
  local oid = source.kind == "commit" and source.sha
    or source.kind == "index_file" and source.blob_oid
  if
    source.kind ~= "commit"
    and source.kind ~= "index_file"
    and source.kind ~= "working_file"
    and source.kind ~= "local_file"
  then
    return false
  end
  if source.kind == "commit" or source.kind == "index_file" then
    return type(oid) == "string" and (#oid == 40 or #oid == 64) and oid:match("^%x+$") ~= nil
  end
  return true
end

local function valid_evidence(evidence)
  return evidence
    and valid_source(evidence.source)
    and type(evidence.selected_text) == "string"
    and #evidence.selected_text <= 65536
    and type(evidence.context_before) == "string"
    and #evidence.context_before <= 8192
    and type(evidence.context_after) == "string"
    and #evidence.context_after <= 8192
end

local function nonblank(value)
  local remaining = value:gsub("[%s]", "")
  for _, codepoint in ipairs({
    0x85,
    0xA0,
    0x1680,
    0x2000,
    0x2001,
    0x2002,
    0x2003,
    0x2004,
    0x2005,
    0x2006,
    0x2007,
    0x2008,
    0x2009,
    0x200A,
    0x2028,
    0x2029,
    0x202F,
    0x205F,
    0x3000,
  }) do
    remaining = remaining:gsub(vim.fn.nr2char(codepoint), "")
  end
  return remaining ~= ""
end

function M.matching_range(evidence, content)
  if
    not nonblank(evidence.selected_text)
    and not nonblank(evidence.context_before)
    and not nonblank(evidence.context_after)
  then
    return nil
  end
  local lines = M.lines(content)
  local selected = vim.split(evidence.selected_text, "\n", { plain = true })
  local before = evidence.context_before == "" and {}
    or vim.split(evidence.context_before, "\n", { plain = true })
  local after = evidence.context_after == "" and {}
    or vim.split(evidence.context_after, "\n", { plain = true })
  local function ordered(first, last, expected)
    local next_line = 1
    for index = first, last do
      if expected[next_line] == lines[index] then
        next_line = next_line + 1
      end
    end
    return next_line > #expected
  end
  local found
  for first = 1, #lines - #selected + 1 do
    local matches = true
    for offset, text in ipairs(selected) do
      if lines[first + offset - 1] ~= text then
        matches = false
        break
      end
    end
    if
      matches
      and ordered(math.max(1, first - #before - 8), first - 1, before)
      and ordered(first + #selected, math.min(#lines, first + #selected + #after + 7), after)
    then
      if found then
        return nil
      end
      found = { first, first + #selected - 1 }
    end
  end
  return found
end

-- This pure contract is shared with Rust through e2e/common/anchor_cases.json.
function M.place(anchor, evidence, path, source, content, path_confirmed)
  local function fallback(status, reason)
    return { status = status, reason = reason }
  end
  if anchor.kind == "general" then
    return fallback("unverified", "Review-level feedback has no code location")
  end
  if not path_confirmed then
    return fallback("unverified", "File association is not confirmed for this comparison")
  end
  if not source and not content then
    return fallback("orphaned", "File does not exist on the selected side")
  end
  if not source or content == nil then
    return fallback("unverified", "Displayed source content is unavailable")
  end
  if not valid_source(source) or source.content_hash ~= "sha256:" .. vim.fn.sha256(content) then
    return fallback("unverified", "Displayed source identity does not match its bytes")
  end
  if not evidence then
    return fallback("unverified", "Original source evidence is unavailable")
  end
  if not valid_evidence(evidence) then
    return fallback("unverified", "Original source evidence is invalid")
  end
  if
    evidence.source.kind == "working_file"
    and evidence.source.content_hash ~= source.content_hash
  then
    return fallback("orphaned", "Unstaged working source changed")
  end
  local same_source = vim.deep_equal(evidence.source, source)
  if anchor.kind == "file" then
    return {
      status = same_source and anchor.path == path and "exact" or "relocated",
      location = { kind = "file", path = path },
      displayed_source = source,
    }
  end
  local lines = M.lines(content)
  local first, last = tonumber(anchor.start), tonumber(anchor["end"])
  local at_original = evidence.source.content_hash == source.content_hash
    and first
    and last
    and first >= 1
    and last >= first
    and last <= #lines
    and table.concat(lines, "\n", first, last) == evidence.selected_text
  local range = at_original and { first, last } or M.matching_range(evidence, content)
  if not range then
    return fallback("outdated", "Original selection was lost, changed, or became ambiguous")
  end
  return {
    status = at_original and anchor.path == path and same_source and "exact" or "relocated",
    location = {
      kind = "line",
      path = path,
      side = anchor.side,
      start = range[1],
      ["end"] = range[2],
    },
    displayed_source = source,
  }
end

function M.source_from_wire(source)
  if not source then
    return nil
  end
  for field, kind in pairs({
    commit = "commit",
    indexFile = "index_file",
    workingFile = "working_file",
    localFile = "local_file",
  }) do
    if source[field] then
      local value = source[field]
      return {
        kind = kind,
        content_hash = value.contentHash,
        sha = value.sha,
        blob_oid = value.blobOid,
      }
    end
  end
end

function M.from_wire(original)
  local source = M.source_from_wire(original.source)
  local anchor = {
    kind = original.kind or "general",
    path = original.path,
    start = tonumber(original.start),
    ["end"] = tonumber(original["end"]),
    side = original.side,
  }
  local evidence = source
      and original.evidence
      and {
        source = source,
        selected_text = original.evidence.selectedText or "",
        context_before = original.evidence.contextBefore or "",
        context_after = original.evidence.contextAfter or "",
      }
    or nil
  return anchor, evidence
end

-- A current path association wins over an old filename that has since been reused.
function M.path_confirmed(anchor, evidence, current_path, view)
  local logical = view.logical_path or view.path
  if current_path and current_path ~= "" then
    if logical == current_path or view.confirmed_rename and current_path == view.previous_path then
      return true
    end
    return anchor.path == view.path
      and evidence
      and evidence.source.kind == "commit"
      and view.source
      and view.source.kind == "commit"
      and evidence.source.sha == view.source.sha
  end
  return anchor.path == logical
    or anchor.path == view.path
    or view.confirmed_rename and anchor.path == view.previous_path
end

function M.resolve(thread, view)
  local original, evidence = M.from_wire(thread.anchor or {})
  if view.unavailable then
    return { status = "unverified", reason = view.unavailable }
  end
  if
    view.source
    and (
      not valid_source(view.source)
      or view.content == nil
      or view.source.content_hash ~= "sha256:" .. vim.fn.sha256(view.content)
    )
  then
    return { status = "unverified", reason = "Displayed source identity does not match its bytes" }
  end
  local observed = thread.forgeObservation
  local comparison = view.comparison and view.comparison.pullRequest
  local current_comparison = observed
    and comparison
    and observed.base == comparison.baseCommit
    and observed.head == comparison.headCommit
  if observed and current_comparison and not view.historical then
    if observed.outdated then
      return {
        status = "outdated",
        reason = "Outdated on the current PR; original evidence is retained",
      }
    end
    local current = observed.current
    if
      current
      and (current.path == view.logical_path or current.path == view.path)
      and (current.kind == "file" or current.side == view.side)
      and view.source
      and (
        current.kind == "file"
        or view.source.kind == "commit"
          and view.source.sha == (current.side == "old" and observed.base or observed.head)
      )
    then
      local location = M.from_wire(current)
      local lines = M.lines(view.content or "")
      if
        location.kind == "file"
        or (
          location.start
          and location["end"]
          and location.start >= 1
          and location["end"] >= location.start
          and location["end"] <= #lines
        )
      then
        return { status = "exact", location = location, displayed_source = view.source }
      end
      return {
        status = "outdated",
        reason = "Provider location is unavailable in the displayed source",
      }
    end
  end
  return M.place(
    original,
    evidence,
    view.path,
    view.source,
    view.content,
    M.path_confirmed(original, evidence, thread.currentPath, view)
  )
end

return M
