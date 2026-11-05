local Notifications = require("lgtm.ui.notifications")
local Object = require("nui.object")
local PullRequest = require("lgtm.models.pull_request")
local n = require("nui-components")

local PrSelectModal = Object("PrSelectModal")

local active_instance = nil

function PrSelectModal:init(opts)
  opts = opts or {}
  assert(opts.pr_service, "pr_service is required")

  local open_prs = opts.pr_service:get_open_prs()
  local current_selected = opts.pr_service.state:get_value().selected_pr
  local initial_pr = current_selected or open_prs[1]

  self.service = opts.pr_service
  self.origin_win = vim.api.nvim_get_current_win()
  self.on_select = opts.on_select
  self.on_cancel = opts.on_cancel
  self.closed = false

  self.signal = n.create_signal({
    query = "",
    filtered_prs = open_prs,
    selected_pr = initial_pr,
  })

  self.renderer = n.create_renderer({
    width = 104,
    height = 24,
    on_unmount = function()
      if active_instance == self then
        active_instance = nil
      end
    end,
  })
end

function PrSelectModal:mount()
  self.renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        self:confirm()
      end,
    },
    {
      mode = { "n" },
      key = { "<Esc>", "q" },
      handler = function()
        self:cancel_selection()
      end,
    },
  })

  self.renderer:render(function()
    return self:render_body()
  end)

  return self
end

function PrSelectModal:render_body()
  return n.columns(
    {
      flex = 1,
    },
    n.rows(
      {
        size = 48,
      },
      n.text_input({
        id = "lgtm-pr-search-input",
        size = 3,
        max_lines = 1,
        border_label = {
          text = " Filter Open PRs (/ branch, title, #) ",
          align = "left",
        },
        value = self.signal.query,
        on_change = function(value)
          self:update_filter(value)
        end,
      }),
      n.select({
        id = "lgtm-pr-select-list",
        flex = 1,
        autofocus = true,
        multiselect = false,
        border_label = {
          text = " Open Branch Pull Requests ",
          align = "left",
        },
        selected = self.signal.selected_pr:dup():map(function(selected_pr)
          if not selected_pr then
            return { id = "empty" }
          end
          return { id = tostring(selected_pr.number) }
        end),
        data = self.signal.filtered_prs:dup():map(function(prs)
          return self:_build_option_nodes(prs)
        end),
        on_select = function(item)
          local pr = item and self:_find_pr_by_id(item.id)
          if pr then
            self.signal.selected_pr = pr
            self:confirm()
          end
        end,
        on_change = function(item)
          local pr = item and self:_find_pr_by_id(item.id)
          if pr then
            self.signal.selected_pr = pr
          end
        end,
      }),
      n.columns(
        {
          size = 3,
          flex = 0,
        },
        n.button({
          id = "lgtm-pr-open-button",
          label = " [ Open Review (<CR>) ] ",
          on_press = function()
            self:confirm()
          end,
        }),
        n.gap(2),
        n.button({
          id = "lgtm-pr-cancel-button",
          label = " [ Cancel (<Esc>) ] ",
          on_press = function()
            self:cancel_selection()
          end,
        })
      )
    ),
    n.paragraph({
      id = "lgtm-pr-preview",
      flex = 1,
      border_label = {
        text = " Pull Request Description ",
        align = "left",
      },
      is_focusable = true,
      lines = self.signal.selected_pr:dup():map(function(pr)
        return table.concat(PullRequest.format_preview_lines(pr), "\n")
      end),
    })
  )
end

function PrSelectModal:update_filter(query)
  local filtered = self:_filter_prs(query or "")
  local current = self.signal:get_value().selected_pr
  local next_selected = filtered[1]
  if current then
    for _, pr in ipairs(filtered) do
      if pr.number == current.number then
        next_selected = pr
        break
      end
    end
  end
  self.signal.query = query or ""
  self.signal.filtered_prs = filtered
  self.signal.selected_pr = next_selected
  return filtered
end

function PrSelectModal:select_by_number(pr_number)
  local pr = self:_find_pr_by_id(pr_number)
  if pr then
    self.signal.selected_pr = pr
  end
  return pr
end

function PrSelectModal:confirm()
  if self.closed then
    return nil
  end
  local selected = self.signal:get_value().selected_pr
  if not selected then
    Notifications.notify("Select an open pull request first", "warn")
    return nil
  end
  self:_close_window(false)
  if self.on_select then
    self.on_select(selected)
  end
  return selected
end

function PrSelectModal:cancel_selection()
  if self.closed then
    return false
  end
  self:_close_window(true)
  if self.on_cancel then
    self.on_cancel()
  end
  return true
end

function PrSelectModal:_build_option_nodes(prs)
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

function PrSelectModal:_find_pr_by_id(id)
  local target_number = tonumber(id)
  if not target_number then
    return nil
  end
  for _, pr in ipairs(self.service.state:get_value().open_prs or {}) do
    if pr.number == target_number then
      return pr
    end
  end
  return nil
end

function PrSelectModal:_filter_prs(query)
  local open_prs = self.service.state:get_value().open_prs or {}
  local filtered = {}
  for _, pr in ipairs(open_prs) do
    if PullRequest.matches_query(pr, query) then
      filtered[#filtered + 1] = pr
    end
  end
  return filtered
end

function PrSelectModal:_close_window(restore_origin)
  self.closed = true
  if active_instance == self then
    active_instance = nil
  end
  local was_mounted = self.renderer
    and self.renderer.layout
    and self.renderer.layout._
    and self.renderer.layout._.mounted
  pcall(vim.cmd, "stopinsert")
  pcall(function()
    self.renderer:close()
  end)
  if restore_origin and self.origin_win and vim.api.nvim_win_is_valid(self.origin_win) then
    pcall(vim.api.nvim_set_current_win, self.origin_win)
  end
  if not was_mounted then
    vim.schedule(function()
      pcall(vim.cmd, "stopinsert")
      pcall(function()
        self.renderer:close()
      end)
    end)
  end
end

function PrSelectModal.open(opts)
  if active_instance and not active_instance.closed then
    active_instance:cancel_selection()
  end
  local instance = PrSelectModal(opts)
  active_instance = instance
  return instance:mount()
end

function PrSelectModal.set_query(query)
  if not active_instance or active_instance.closed then
    return nil
  end
  return active_instance:update_filter(query)
end

function PrSelectModal.select_number(pr_number)
  if not active_instance or active_instance.closed then
    return nil
  end
  return active_instance:select_by_number(pr_number)
end

function PrSelectModal.confirm_selection()
  if not active_instance then
    return nil
  end
  return active_instance:confirm()
end

function PrSelectModal.cancel()
  if not active_instance then
    return false
  end
  return active_instance:cancel_selection()
end

function PrSelectModal.is_open()
  return active_instance ~= nil and not active_instance.closed
end

function PrSelectModal.get_active()
  return active_instance
end

function PrSelectModal.reset()
  if active_instance and not active_instance.closed then
    active_instance:_close_window(false)
  end
  active_instance = nil
end

return PrSelectModal
