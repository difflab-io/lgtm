local M = {}
local rpc, picker = require("lgtm.rpc"), require("lgtm.picker")
function M.select(completed)
  local reviews = {}
  local function page(token)
    rpc.latest(
      "review-picker",
      "ListReviews",
      { state = completed and "completed" or "active", pageToken = token, pageSize = 100 },
      function(err, result)
        if err then
          require("lgtm.notifications").error(err)
          return
        end
        vim.list_extend(reviews, result.reviews or {})
        if result.nextPageToken and result.nextPageToken ~= "" then
          page(result.nextPageToken)
          return
        end
        picker.searchable("Local / GitHub reviews", reviews, function(review)
          local target = review.target
          local label = target.pullRequest
              and ("PR #" .. target.pullRequest.number .. " · " .. target.pullRequest.repositoryUrl:gsub(
                "^https://github.com/",
                ""
              ))
            or target.localGit and ((target.localGit.branch or ""):gsub("^refs/heads/", "") .. " · " .. vim.fn.fnamemodify(
              target.localGit.path,
              ":~"
            ))
            or vim.fn.fnamemodify(target.directory.path, ":~")
          return string.format(
            "%s [%s] %s",
            label,
            review.state,
            (review.createdAt or ""):gsub("T", " "):sub(1, 16)
          )
        end, function(review)
          require("lgtm.review").open(require("lgtm.context").apply({}, review))
        end)
      end
    )
  end
  page(nil)
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
  local pr_notice = require("lgtm.notifications").show("Loading pull requests…")
  rpc.latest(
    "pr-picker",
    "ListPullRequests",
    { repositoryUrl = url, query = query, page = page },
    function(err, result)
      if err then
        require("lgtm.notifications").error(err, pr_notice)
        return
      end
      pr_notice = require("lgtm.notifications").show("Pull requests ready", nil, pr_notice)
      local entries = result.pullRequests or {}
      if (result.nextPage or 0) > 0 then
        entries[#entries + 1] = { nextPage = result.nextPage }
      end
      picker.searchable("Pull requests", entries, function(pr)
        return pr.nextPage and "Next page…" or ("#" .. pr.number .. " " .. pr.title)
      end, function(pr)
        if pr.nextPage then
          M.pr_page(url, query, pr.nextPage)
          return
        end
        local contexts = require("lgtm.context")
        local state = {
          target = { pullRequest = { repositoryUrl = url, number = pr.number } },
          comparison = { pullRequest = { baseCommit = pr.base, headCommit = pr.head } },
          base = pr.base,
          head = pr.head,
          mode = "pr",
          kind = "pr",
          initializing = true,
        }
        local notice = require("lgtm.notifications")
        local pending = notice.show("Loading PR comments…", nil, pr_notice)
        -- The native tree opens from the picker metadata while the backend loads comments.
        require("lgtm.review").open(state)
        local function start(checkout)
          contexts.start(state.target, state.comparison, nil, function(error, loaded)
            if contexts.current ~= state then
              return
            end
            if error then
              state.load_error = error
              notice.error(error, pending)
              return
            end
            local old_base, old_head = state.base, state.head
            contexts.apply(state, loaded.review)
            state.initializing = false
            if old_base ~= state.base or old_head ~= state.head then
              require("lgtm.codediff").close(state)
              state.tab, state.browsing, state.native_explorer = nil, nil, nil
            end
            local function ready(refresh_error)
              if refresh_error then
                notice.error(refresh_error, pending)
              else
                notice.show("Review ready", nil, pending)
              end
            end
            if not state.browsing and not state.git_pending then
              require("lgtm.review").open(state, ready)
            else
              state.refresh(ready)
            end
          end, { localGitPath = checkout })
        end
        vim.system(
          { "git", "-C", vim.fn.getcwd(), "rev-parse", "--show-toplevel" },
          { text = true },
          function(result)
            vim.schedule(function()
              start(result.code == 0 and vim.trim(result.stdout) or nil)
            end)
          end
        )
      end)
    end
  )
end
return M
