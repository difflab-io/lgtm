-- State -----------------------------------------------------------------------

local M = {}
local UI, Private = {}, {}
local picker = require("lgtm.ui.picker")
local notice = require("lgtm.ui.notifications")

-- UI --------------------------------------------------------------------------

UI.search = require("lgtm.ui.search").open

-- Lifecycle -------------------------------------------------------------------

function M.open(repository_url)
  local function select_remote(remote)
    require("lgtm.services.forge_service").get_pr_context(vim.fn.getcwd(), {
      repository_url = repository_url,
      remote = remote,
    }, function(context)
      if context.repository_url then
        M.pr_page(
          context.repository_url,
          context.branch ~= "" and ("head:" .. context.branch) or "",
          1
        )
      elseif #context.remotes > 1 then
        picker.select("Select Git remote", context.remotes, tostring, select_remote)
      else
        picker.input("Repository URL (no GitHub remote found): ", "https://github.com/", M.open)
      end
    end)
  end
  select_remote()
end

-- Methods ---------------------------------------------------------------------

function M.select_base(callback)
  picker.input("Base branch: ", "main", callback)
end

function M.pr_page(url, query, page)
  local sessions = require("lgtm.services.session_service")
  local pending = notice.show("Loading pull requests…")
  UI.search({
    title = "Pull requests",
    data = sessions.get_catalog():map(function(data)
      local items = {}
      for index, pr in ipairs(data.pull_requests) do
        items[index] = {
          id = tostring(index),
          label = pr.nextPage and "Next page…" or ("#" .. pr.number .. " " .. pr.title),
        }
      end
      return items
    end),
    on_select = function(id)
      local pr = sessions.get_catalog():get_value().pull_requests[tonumber(id)]
      if pr.nextPage then
        M.pr_page(url, query, pr.nextPage)
      else
        Private.open_pr(url, pr, pending)
      end
    end,
  })
  sessions.get_pull_requests(url, query, page, function(err)
    if err then
      notice.error(err, pending)
    else
      pending = notice.show("Pull requests ready", nil, pending)
    end
  end)
end

-- Private ---------------------------------------------------------------------

function Private.open_pr(repository_url, pr, pr_notice)
  local pending = notice.show("Loading PR comments…", nil, pr_notice)
  local state = require("lgtm.services.session_service").create_pr(
    repository_url,
    pr,
    function(err, loaded, comparison_changed)
      if err then
        notice.error(err, pending)
        return
      end
      if comparison_changed then
        require("lgtm.features.diff_view").close(loaded)
        loaded.tab, loaded.browsing, loaded.native_explorer = nil, nil, nil
      end
      local function ready(refresh_error)
        if refresh_error then
          notice.error(refresh_error, pending)
        else
          notice.show(
            loaded.resume_notice or "Review ready",
            loaded.resume_notice and vim.log.levels.WARN or nil,
            pending
          )
        end
      end
      if not loaded.browsing and not loaded.git_pending then
        require("lgtm.app").open(loaded, ready)
      else
        loaded.refresh(ready)
      end
    end
  )
  state.session_subscription = require("lgtm.services.session_service")
    .get_data(state)
    :get_observable()
    :subscribe(function(data)
      if not data.loading then
        return
      end
      pending = notice.show("Loading PR comments…", nil, pending)
    end)
  -- Open native buffers immediately while the session service loads the discussion.
  require("lgtm.app").open(state)
end
return M
