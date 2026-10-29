-- Normalize backend source identities and saved text for native buffer checks.
-- State -----------------------------------------------------------------------

local M = {}

-- Match Rust str::lines(): CRLF is one ending; final newlines are not extra lines.

-- Methods ---------------------------------------------------------------------

function M.lines(bytes)
  local lines, start = {}, 1
  while true do
    local ending = bytes:find("\n", start, true)
    if not ending then
      if start <= #bytes then
        lines[#lines + 1] = bytes:sub(start)
      end
      return lines
    end
    local line = bytes:sub(start, ending - 1)
    if line:sub(-1) == "\r" then
      line = line:sub(1, -2)
    end
    lines[#lines + 1] = line
    start = ending + 1
  end
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

function M.source_to_wire(source)
  local value = { contentHash = source.content_hash }
  if source.kind == "commit" then
    value.sha = source.sha
  elseif source.kind == "index_file" then
    value.blobOid = source.blob_oid
  end
  local field = ({
    commit = "commit",
    index_file = "indexFile",
    working_file = "workingFile",
    local_file = "localFile",
  })[source.kind]
  return { [field] = value }
end
return M
