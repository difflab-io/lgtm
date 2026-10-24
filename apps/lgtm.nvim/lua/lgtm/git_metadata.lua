-- Compatibility for CodeDiff's pinned C-quoted filename decoder. Content stays in CodeDiff.
-- Normalize only suspicious metadata in an active LGTM checkout, using literal NUL-separated paths.
-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local installed = false

-- Public ----------------------------------------------------------------------

function M.get_status(root, base, head, staged, pathspec, callback)
  local comparison = staged and { "--cached", base } or head and { base, head } or { base }
  local args = { "diff", "--name-status", "-M", "-z" }
  vim.list_extend(args, comparison)
  args[#args + 1] = "--"
  vim.list_extend(args, pathspec or {})
  Private.run(root, args, function(err, output)
    if err then
      callback(err)
      return
    end
    local result = { unstaged = {}, staged = {}, conflicts = {} }
    local entries = result[staged and "staged" or "unstaged"]
    local records = vim.split(output, "\0", { plain = true, trimempty = true })
    local index = 1
    while index <= #records do
      local status, path, previous = records[index], records[index + 1], nil
      index = index + 2
      if status:match("^[RC]") then
        previous, path, index = path, records[index], index + 1
      end
      if not path then
        callback("Incomplete NUL-separated Git file metadata")
        return
      end
      entries[#entries + 1] = { status = status:sub(1, 1), path = path, old_path = previous }
    end
    local function stats()
      local arguments = { "diff", "--numstat", "-M", "-z" }
      vim.list_extend(arguments, comparison)
      arguments[#arguments + 1] = "--"
      vim.list_extend(arguments, pathspec or {})
      Private.run(root, arguments, function(stats_error, data)
        if stats_error then
          callback(stats_error)
          return
        end
        local fields, counts, cursor = vim.split(data, "\0", { plain = true }), {}, 1
        while cursor <= #fields do
          local added, removed, path = fields[cursor]:match("^([^\t]+)\t([^\t]+)\t(.*)$")
          if added then
            if path == "" then
              path, cursor = fields[cursor + 2], cursor + 2
            end
            if path then
              counts[path] = {
                insertions = tonumber(added) or 0,
                deletions = tonumber(removed) or 0,
                binary = added == "-",
              }
            end
          end
          cursor = cursor + 1
        end
        for _, entry in ipairs(entries) do
          entry.line_stats = counts[entry.path]
        end
        callback(nil, result)
      end)
    end
    local mode = (require("codediff.config").options.explorer or {}).untracked or "all"
    if staged or head or mode == "no" then
      stats()
      return
    end
    local untracked = { "ls-files", "--others", "--exclude-standard", "-z" }
    if mode == "normal" then
      untracked[#untracked + 1] = "--directory"
    end
    untracked[#untracked + 1] = "--"
    vim.list_extend(untracked, pathspec or {})
    Private.run(root, untracked, function(untracked_error, data)
      if untracked_error then
        callback(untracked_error)
        return
      end
      for path in data:gmatch("([^%z]+)%z") do
        entries[#entries + 1] = { status = "??", path = path }
      end
      stats()
    end)
  end)
end

function M.install()
  if installed then
    return
  end
  installed = true
  local git = require("codediff.core.git")
  for _, definition in ipairs({
    { name = "get_diff_staged", refs = 1, staged = true },
    { name = "get_diff_revision_with_line_stats", refs = 1 },
    { name = "get_diff_revisions_with_line_stats", refs = 2 },
  }) do
    local spec, original = definition, git[definition.name]
    git[spec.name] = function(...)
      local args, count = { ... }, select("#", ...)
      local root_index, callback_index = spec.refs + 1, spec.refs + 2
      local state = require("lgtm.services.state").current
      if not state or state.git_root ~= args[root_index] then
        return original(unpack(args, 1, count))
      end
      local callback = args[callback_index]
      args[callback_index] = function(err, result)
        if err or not Private.suspicious(result) then
          callback(err, result)
          return
        end
        M.get_status(
          args[root_index],
          args[1],
          spec.refs == 2 and args[2] or nil,
          spec.staged,
          args[callback_index + 1],
          callback
        )
      end
      return original(unpack(args, 1, count))
    end
  end
end

-- Private ---------------------------------------------------------------------

function Private.run(root, arguments, callback)
  local command = { "git", "--no-optional-locks", "--literal-pathspecs", "-C", root }
  vim.list_extend(command, arguments)
  vim.system(command, { text = false, timeout = 5000 }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        callback(result.stderr or "Git metadata is unavailable")
      else
        callback(nil, result.stdout or "")
      end
    end)
  end)
end

function Private.suspicious(result)
  for _, group in pairs(result or {}) do
    if type(group) == "table" then
      for _, entry in ipairs(group) do
        for _, path in ipairs({ entry.path or "", entry.old_path or "" }) do
          if path:match("[23][0-7][0-7]") or path:sub(1, 1) == '"' then
            return true
          end
        end
      end
    end
  end
  return false
end

return M
