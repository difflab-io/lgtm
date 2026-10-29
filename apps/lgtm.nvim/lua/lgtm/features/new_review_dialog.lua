-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local picker = require("lgtm.ui.picker")
local sessions = require("lgtm.services.session_service")
local notice = require("lgtm.ui.notifications")

-- Lifecycle -------------------------------------------------------------------

function M.open()
  picker.select("Review", sessions.get_options(), tostring, function(kind)
    if kind == "PR" then
      require("lgtm.features.branch_pr_selector").open()
    else
      M.local_review(kind)
    end
  end)
end

-- Methods ---------------------------------------------------------------------

function M.local_review(mode)
  local function create(selected)
    local function start(base)
      sessions.create_local(vim.fn.getcwd(), selected, base, function(err, state)
        if err then
          notice.error(err)
        else
          require("lgtm.app").open(state)
        end
      end, { on_conflict = Private.resolve_comparison })
    end
    if selected == "branch" then
      require("lgtm.features.branch_pr_selector").select_base(start)
    else
      start()
    end
  end
  if mode then
    create(mode)
  else
    picker.select("Review content", { "working", "staged" }, tostring, create)
  end
end

function M.directory(path)
  local function start(selected)
    local root = vim.fn.fnamemodify(selected, ":p"):gsub("/$", "")
    require("lgtm.services.session_service").create(
      { directory = { path = root } },
      { directory = vim.empty_dict() },
      root,
      function(err, state)
        if err then
          notice.error(err)
        else
          require("lgtm.app").open(state)
        end
      end
    )
  end
  if path then
    start(path)
  else
    require("lgtm.ui.picker").input(
      "Review directory (includes ignored files): ",
      vim.fn.getcwd(),
      start
    )
  end
end

-- Private ---------------------------------------------------------------------

function Private.resolve_comparison(_, done)
  require("lgtm.ui.picker").select(
    "Saved review uses a different comparison",
    { "Resume saved review", "Complete saved review and start this comparison" },
    tostring,
    done
  )
end
return M
