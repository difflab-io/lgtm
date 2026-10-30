-- State -----------------------------------------------------------------------

local M = {}
local UI, Private = {}, {}
local sessions = require("lgtm.services.session_service")

-- Render ----------------------------------------------------------------------

function M.render()
  UI.search({
    title = "Local / GitHub reviews",
    data = sessions.get_catalog():map(function(data)
      return vim.tbl_map(function(review)
        return { id = review.id, label = Private.label(review) }
      end, data.reviews)
    end),
    on_select = function(id)
      for _, review in ipairs(sessions.get_catalog():get_value().reviews) do
        if review.id == id then
          require("lgtm.app").open(require("lgtm.services.state").apply({}, review))
          return
        end
      end
    end,
  })
end

-- UI --------------------------------------------------------------------------

UI.search = require("lgtm.ui.search").open

-- Lifecycle -------------------------------------------------------------------

function M.select(completed)
  sessions.get_sessions(completed, function(err)
    if err then
      require("lgtm.ui.notifications").error(err)
    else
      M.render()
    end
  end)
end

-- Private ---------------------------------------------------------------------

function Private.label(review)
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
end
return M
