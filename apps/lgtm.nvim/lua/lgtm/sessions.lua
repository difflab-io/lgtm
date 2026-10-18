local M = {}
local rpc, picker = require("lgtm.rpc"), require("lgtm.picker")
function M.select(completed)
  picker.input("Filter sessions by path (empty for all): ", "", function(path)
    rpc.latest(
      "session-picker",
      "ListSessions",
      { completed = completed or false, path = path },
      function(err, result)
        if err then
          require("lgtm.notifications").error(err)
          return
        end
        picker.select("Local / GitHub sessions", result.entries or {}, function(entry)
          local key = entry.context.key
          return string.format(
            "%s [%s] %s",
            key.path or key.repositoryUrl or key.gitDir or "",
            entry.session.state,
            entry.session.id
          )
        end, function(entry)
          local state = {
            context = entry.context,
            session = entry.session,
            root = entry.context.paths and entry.context.paths[1]
              or entry.context.key.path
              or entry.context.key.gitDir,
          }
          if entry.context.key.kind == "branch" then
            picker.select(
              "Resume comparison",
              { "working", "staged", "revisions" },
              tostring,
              function(mode)
                state.mode = mode
                require("lgtm.review").open(state)
              end
            )
          else
            require("lgtm.review").open(state)
          end
        end)
      end
    )
  end)
end

function M.prs(repository_url)
  local function search(url)
    picker.input("Search pull requests: ", "", function(query)
      M.pr_page(url, query, 1)
    end)
  end
  if repository_url then
    search(repository_url)
  else
    picker.input("Repository URL: ", "https://github.com/", search)
  end
end

function M.pr_page(url, query, page)
  local notice = require("lgtm.notifications").show("Loading pull requests…")
  rpc.latest(
    "pr-picker",
    "ListPullRequests",
    { repositoryUrl = url, query = query, page = page },
    function(err, result)
      if err then
        require("lgtm.notifications").error(err, notice)
        return
      end
      local entries = result.pullRequests or {}
      if (result.nextPage or 0) > 0 then
        entries[#entries + 1] = { nextPage = result.nextPage }
      end
      picker.select("Pull requests", entries, function(pr)
        return pr.nextPage and "Next page…" or ("#" .. pr.number .. " " .. pr.title)
      end, function(pr)
        if pr.nextPage then
          M.pr_page(url, query, pr.nextPage)
          return
        end
        rpc.call(
          "EnsureContext",
          { key = { kind = "forge_backed", repositoryUrl = url, number = pr.number } },
          function(error, response)
            if error then
              require("lgtm.notifications").error(error)
              return
            end
            rpc.call(
              "RefreshContext",
              { contextId = response.context.id },
              function(refresh_err, refreshed)
                if refresh_err then
                  require("lgtm.notifications").error(refresh_err)
                  return
                end
                rpc.call("StartSession", {
                  contextId = response.context.id,
                  base = refreshed.context.refreshedBase,
                  head = refreshed.context.refreshedHead,
                }, function(start_err, session)
                  if start_err then
                    require("lgtm.notifications").error(start_err)
                    return
                  end
                  require("lgtm.review").open({
                    context = refreshed.context,
                    session = session.session,
                  })
                end)
              end
            )
          end
        )
      end)
    end
  )
end
return M
