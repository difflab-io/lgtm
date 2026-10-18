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
            picker.select("Resume comparison", { "working", "staged" }, tostring, function(mode)
              state.mode = mode
              require("lgtm.review").open(state)
            end)
          else
            require("lgtm.review").open(state)
          end
        end)
      end
    )
  end)
end

-- Git remotes may use HTTPS, SCP-style SSH, or ssh:// addresses.
function M.repository_url(remote)
  local path = vim.trim(remote):gsub("/+$", ""):gsub("%.git$", "")
  local repository = path:match("^https?://github%.com/(.+)$")
    or path:match("^[^@]+@github%.com:(.+)$")
    or path:match("^ssh://[^@]+@github%.com/(.+)$")
  if repository and repository:match("^[^/]+/[^/]+$") then
    return "https://github.com/" .. repository
  end
end

function M.prs(repository_url)
  local function open(url)
    vim.system(
      { "git", "-C", vim.fn.getcwd(), "branch", "--show-current" },
      { text = true },
      function(result)
        vim.schedule(function()
          local branch = result.code == 0 and vim.trim(result.stdout) or ""
          M.pr_page(url, branch ~= "" and ("head:" .. branch) or "", 1)
        end)
      end
    )
  end
  local function fallback()
    picker.input("Repository URL (no GitHub remote found): ", "https://github.com/", open)
  end
  if repository_url and repository_url ~= "" then
    open(M.repository_url(repository_url) or repository_url)
    return
  end
  local cwd = vim.fn.getcwd()
  local function remote(name, callback)
    vim.system({ "git", "-C", cwd, "remote", "get-url", name }, { text = true }, function(result)
      vim.schedule(function()
        callback(result.code == 0 and M.repository_url(result.stdout) or nil)
      end)
    end)
  end
  remote("origin", function(url)
    if url then
      open(url)
      return
    end
    vim.system({ "git", "-C", cwd, "remote" }, { text = true }, function(result)
      vim.schedule(function()
        local remotes = result.code == 0
            and vim.split(vim.trim(result.stdout), "\n", { trimempty = true })
          or {}
        local function select(name)
          remote(name, function(found)
            if found then
              open(found)
            else
              fallback()
            end
          end)
        end
        if #remotes == 1 then
          select(remotes[1])
        elseif #remotes > 1 then
          picker.select("Select Git remote", remotes, tostring, select)
        else
          fallback()
        end
      end)
    end)
  end)
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
