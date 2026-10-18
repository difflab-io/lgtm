local M = { current = nil, generation = 0 }
function M.set(state)
  M.generation = M.generation + 1
  M.current = state
end

function M.ensure(key, root, callback, commits)
  require("lgtm.rpc").call(
    "EnsureContext",
    { key = key, workspacePath = root },
    function(err, result)
      if err then
        callback(err)
        return
      end
      require("lgtm.rpc").call("StartSession", {
        contextId = result.context.id,
        base = commits and commits.base or result.context.refreshedBase,
        head = commits and commits.head or result.context.refreshedHead,
      }, function(error, response)
        callback(
          error,
          not error and { context = result.context, session = response.session, root = root } or nil
        )
      end)
    end
  )
end

function M.notes(path, callback)
  local root = vim.fn.fnamemodify(path, ":p"):gsub("/$", "")
  M.ensure({ kind = "notes", path = root }, root, callback)
end
return M
