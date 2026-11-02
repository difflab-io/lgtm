local Notifications = require("lgtm.ui.notifications")
local PullRequest = require("lgtm.models.pull_request")
local n = require("nui-components")

local PrSelectModal = {}
local UI = {}
local Private = {}

local active_session = nil

-- State -----------------------------------------------------------------------

local function create_state(service)
  local open_prs = service:get_open_prs()
  local current_selected = service.state:get_value().selected_pr
  local initial_pr = current_selected or open_prs[1]
  return n.create_signal({
    query = "",
    filtered_prs = open_prs,
    selected_pr = initial_pr,
  })
end

-- Render ----------------------------------------------------------------------

local function render(session)
  local renderer = n.create_renderer({
    width = 104,
    height = 24,
  })

  renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        Private.confirm(session)
      end,
    },
    {
      mode = { "n" },
      key = "<Esc>",
      handler = function()
        Private.cancel(session)
      end,
    },
    {
      mode = { "n" },
      key = "q",
      handler = function()
        Private.cancel(session)
      end,
    },
  })

  renderer:on_unmount(function()
    if active_session == session then
      active_session = nil
    end
  end)

  session.renderer = renderer
  renderer:render(function()
    return n.columns(
      {
        flex = 1,
      },
      n.rows({
        size = 48,
      }, UI.search_input(session), UI.pr_list(session), UI.action_buttons(session)),
      UI.preview_pane(session)
    )
  end)
end

-- UI --------------------------------------------------------------------------

function UI.search_input(session)
  return n.text_input({
    id = "lgtm-pr-search-input",
    size = 3,
    max_lines = 1,
    border_label = {
      text = " Filter Open PRs (/ branch, title, #) ",
      align = "left",
    },
    value = session.signal.query,
    on_change = function(value)
      Private.update_filter(session, value)
    end,
  })
end

function UI.pr_list(session)
  local list_signal = session.signal.filtered_prs:dup()
  return n.select({
    id = "lgtm-pr-select-list",
    flex = 1,
    autofocus = true,
    multiselect = false,
    border_label = {
      text = " Open Branch Pull Requests ",
      align = "left",
    },
    selected = session.signal.selected_pr:dup():map(function(selected_pr)
      if not selected_pr then
        return { id = "empty" }
      end
      return { id = tostring(selected_pr.number) }
    end),
    data = list_signal:map(function(prs)
      return Private.build_option_nodes(prs)
    end),
    on_select = function(item)
      local pr = item and Private.find_pr_by_id(session.service, item.id)
      if pr then
        session.signal.selected_pr = pr
        Private.confirm(session)
      end
    end,
    on_change = function(item)
      local pr = item and Private.find_pr_by_id(session.service, item.id)
      if pr then
        session.signal.selected_pr = pr
      end
    end,
  })
end

function UI.action_buttons(session)
  return n.columns(
    {
      size = 3,
      flex = 0,
    },
    n.button({
      id = "lgtm-pr-open-button",
      label = " [ Open Review (<CR>) ] ",
      on_press = function()
        Private.confirm(session)
      end,
    }),
    n.gap(2),
    n.button({
      id = "lgtm-pr-cancel-button",
      label = " [ Cancel (<Esc>) ] ",
      on_press = function()
        Private.cancel(session)
      end,
    })
  )
end

function UI.preview_pane(session)
  local preview_signal = session.signal.selected_pr:dup()
  return n.paragraph({
    id = "lgtm-pr-preview",
    flex = 1,
    border_label = {
      text = " Pull Request Description ",
      align = "left",
    },
    is_focusable = true,
    lines = preview_signal:map(function(pr)
      return table.concat(PullRequest.format_preview_lines(pr), "\n")
    end),
  })
end

-- Methods ---------------------------------------------------------------------

function PrSelectModal.open(opts)
  opts = opts or {}
  assert(opts.pr_service, "pr_service is required")
  if active_session and not active_session.closed then
    Private.cancel(active_session)
  end

  local session = {
    service = opts.pr_service,
    signal = create_state(opts.pr_service),
    origin_win = vim.api.nvim_get_current_win(),
    on_select = opts.on_select,
    on_cancel = opts.on_cancel,
    closed = false,
  }
  active_session = session
  render(session)
  return session
end

function PrSelectModal.set_query(query)
  if not active_session or active_session.closed then
    return nil
  end
  Private.update_filter(active_session, query)
  return active_session.signal:get_value().filtered_prs
end

function PrSelectModal.select_number(pr_number)
  if not active_session or active_session.closed then
    return nil
  end
  local pr = Private.find_pr_by_id(active_session.service, pr_number)
  if pr then
    active_session.signal.selected_pr = pr
  end
  return pr
end

function PrSelectModal.confirm_selection()
  if not active_session then
    return nil
  end
  return Private.confirm(active_session)
end

function PrSelectModal.cancel()
  if not active_session then
    return false
  end
  return Private.cancel(active_session)
end

function PrSelectModal.is_open()
  return active_session ~= nil and not active_session.closed
end

function PrSelectModal.get_active()
  return active_session
end

function PrSelectModal.reset()
  if active_session and not active_session.closed then
    active_session.closed = true
    pcall(function()
      active_session.renderer:close()
    end)
  end
  active_session = nil
end

-- Private ---------------------------------------------------------------------

function Private.build_option_nodes(prs)
  if #prs == 0 then
    return {
      n.option("No matching open pull requests", {
        id = "empty",
      }),
    }
  end
  local nodes = {}
  for _, pr in ipairs(prs) do
    nodes[#nodes + 1] = n.option(PullRequest.format_list_label(pr), {
      id = tostring(pr.number),
    })
  end
  return nodes
end

function Private.find_pr_by_id(service, id)
  local target_number = tonumber(id)
  if not target_number then
    return nil
  end
  for _, pr in ipairs(service.state:get_value().open_prs or {}) do
    if pr.number == target_number then
      return pr
    end
  end
  return nil
end

function Private.filter_prs(service, query)
  local open_prs = service.state:get_value().open_prs or {}
  local filtered = {}
  for _, pr in ipairs(open_prs) do
    if PullRequest.matches_query(pr, query) then
      filtered[#filtered + 1] = pr
    end
  end
  return filtered
end

function Private.update_filter(session, query)
  local filtered = Private.filter_prs(session.service, query or "")
  local current = session.signal:get_value().selected_pr
  local next_selected = filtered[1]
  if current then
    for _, pr in ipairs(filtered) do
      if pr.number == current.number then
        next_selected = pr
        break
      end
    end
  end
  session.signal.query = query or ""
  session.signal.filtered_prs = filtered
  session.signal.selected_pr = next_selected
end

function Private.close_session(session, restore_origin)
  session.closed = true
  if active_session == session then
    active_session = nil
  end
  local was_mounted = session.renderer
    and session.renderer.layout
    and session.renderer.layout._
    and session.renderer.layout._.mounted
  pcall(vim.cmd, "stopinsert")
  pcall(function()
    session.renderer:close()
  end)
  if restore_origin and session.origin_win and vim.api.nvim_win_is_valid(session.origin_win) then
    pcall(vim.api.nvim_set_current_win, session.origin_win)
  end
  if not was_mounted then
    vim.schedule(function()
      pcall(vim.cmd, "stopinsert")
      pcall(function()
        session.renderer:close()
      end)
    end)
  end
end

function Private.confirm(session)
  if session.closed then
    return nil
  end
  local selected = session.signal:get_value().selected_pr
  if not selected then
    Notifications.notify("Select an open pull request first", "warn")
    return nil
  end
  Private.close_session(session, false)
  if session.on_select then
    session.on_select(selected)
  end
  return selected
end

function Private.cancel(session)
  if session.closed then
    return false
  end
  Private.close_session(session, true)
  if session.on_cancel then
    session.on_cancel()
  end
  return true
end

return PrSelectModal
