local Object = require("nui.object")
local ReviewSubmission = require("lgtm.models.review_submission")
local n = require("nui-components")

local PrSubmitEditor = Object("PrSubmitEditor")

local active_instance = nil

function PrSubmitEditor:init(opts)
  assert(type(opts) == "table" and opts.pr, "PrSubmitEditor requires an active PR")
  local initial_outcome = self:_normalize_outcome(opts.initial_outcome)
  local initial_body = opts.initial_body or ""

  self.pr = opts.pr
  self.origin_win = vim.api.nvim_get_current_win()
  self.on_submit = assert(opts.on_submit, "PrSubmitEditor requires on_submit")
  self.on_cancel = opts.on_cancel
  self.closed = false

  self.signal = n.create_signal({
    outcome = initial_outcome,
    selected_outcome = {
      id = initial_outcome,
      text = ReviewSubmission.outcome_label(initial_outcome),
    },
    body = initial_body,
    error = "",
  })

  self.renderer = n.create_renderer({
    width = 80,
    height = 18,
    on_unmount = function()
      if active_instance == self then
        active_instance = nil
      end
    end,
  })
end

function PrSubmitEditor:mount()
  self.renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        self:submit_review()
      end,
    },
    {
      mode = { "n" },
      key = { "<Esc>", "q" },
      handler = function()
        self:cancel_review()
      end,
    },
  })

  self.renderer:render(function()
    return self:render_body()
  end)

  return self
end

function PrSubmitEditor:render_body()
  local has_error = self.signal.error:dup():map(function(err)
    return type(err) == "string" and err ~= ""
  end)

  return n.form(
    {
      id = "pr_submit_form",
      submit_key = "<C-s>",
      on_submit = function(is_valid)
        if is_valid then
          self:submit_review()
        end
      end,
    },
    n.paragraph({
      lines = string.format(
        "Submit Review for PR #%d · %s (@%s)",
        self.pr.number,
        self.pr.title,
        self.pr.author
      ),
      align = "left",
      is_focusable = false,
      padding = {
        top = 0,
        bottom = 0,
        left = 1,
        right = 1,
      },
    }),
    n.select({
      id = "pr_submit_outcome_select",
      size = 5,
      border_label = {
        text = " Review Outcome ",
        align = "left",
      },
      selected = self.signal.selected_outcome,
      data = {
        n.option("Comment", {
          id = "comment",
        }),
        n.option("Approve", {
          id = "approve",
        }),
        n.option("Request Changes", {
          id = "request_changes",
        }),
      },
      multiselect = false,
      on_select = function(node)
        self.signal.selected_outcome = node
        self.signal.outcome = node.id
        self.signal.error = ""
      end,
    }),
    n.text_input({
      id = "pr_submit_body_input",
      flex = 1,
      autofocus = true,
      wrap = true,
      border_label = {
        text = " Overall Review Summary ",
        align = "left",
      },
      value = self.signal.body,
      on_change = function(value)
        self.signal.body = value
        self.signal.error = ""
      end,
    }),
    n.paragraph({
      lines = self.signal.error,
      hidden = has_error:negate(),
      is_focusable = false,
      padding = {
        top = 0,
        bottom = 0,
        left = 1,
        right = 1,
      },
    }),
    n.columns(
      {
        size = 3,
        flex = 0,
      },
      n.gap({
        flex = 1,
      }),
      n.button({
        id = "pr_submit_btn",
        label = " [ Submit Review (<C-s>) ] ",
        on_press = function()
          self:submit_review()
        end,
      }),
      n.gap(2),
      n.button({
        id = "pr_cancel_btn",
        label = " [ Cancel (<Esc>) ] ",
        on_press = function()
          self:cancel_review()
        end,
      }),
      n.gap(1)
    )
  )
end

function PrSubmitEditor:set_outcome(outcome)
  local normalized = self:_normalize_outcome(outcome)
  self.signal.outcome = normalized
  self.signal.selected_outcome = {
    id = normalized,
    text = ReviewSubmission.outcome_label(normalized),
  }
end

function PrSubmitEditor:update_body(text)
  self.signal.body = text
  local input_cmp = self.renderer:get_component_by_id("pr_submit_body_input")
  if input_cmp then
    input_cmp:set_current_value(text)
    if input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
      local lines = vim.split(text or "", "\n", { plain = true })
      vim.api.nvim_buf_set_lines(input_cmp.bufnr, 0, -1, false, lines)
    end
  end
end

function PrSubmitEditor:submit_review()
  if self.closed then
    return false
  end
  local snapshot = self.signal:get_value()
  local outcome = snapshot.outcome or "comment"
  local body = self:_read_current_body()

  if (outcome == "comment" or outcome == "request_changes") and body == "" then
    self.signal.error = string.format(
      "Review summary body is required when outcome is %s.",
      ReviewSubmission.outcome_label(outcome)
    )
    return false
  end

  self:_close_window()
  self.on_submit({
    outcome = outcome,
    body = body,
  })
  return true
end

function PrSubmitEditor:cancel_review()
  if self.closed then
    return
  end
  self:_close_window()
  if self.on_cancel then
    self.on_cancel()
  end
end

function PrSubmitEditor:_read_current_body()
  local input_cmp = self.renderer:get_component_by_id("pr_submit_body_input")
  if input_cmp then
    local val = input_cmp:get_current_value()
    if type(val) == "string" and vim.trim(val) ~= "" then
      return vim.trim(val)
    end
    if input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
      local lines = vim.api.nvim_buf_get_lines(input_cmp.bufnr, 0, -1, false)
      local from_buf = vim.trim(table.concat(lines, "\n"))
      if from_buf ~= "" then
        return from_buf
      end
    end
  end
  local body = self.signal:get_value().body
  return type(body) == "string" and vim.trim(body) or ""
end

function PrSubmitEditor:_close_window()
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
  if self.origin_win and vim.api.nvim_win_is_valid(self.origin_win) then
    pcall(vim.api.nvim_set_current_win, self.origin_win)
  end
  if not was_mounted then
    vim.schedule(function()
      pcall(vim.cmd, "stopinsert")
      pcall(function()
        self.renderer:close()
      end)
      local cur_cfg = vim.api.nvim_win_get_config(0)
      if
        cur_cfg.relative ~= ""
        and self.origin_win
        and vim.api.nvim_win_is_valid(self.origin_win)
      then
        pcall(vim.api.nvim_set_current_win, self.origin_win)
      end
    end)
  end
end

function PrSubmitEditor:_normalize_outcome(outcome)
  local value = (type(outcome) == "string" and vim.trim(outcome) or ""):lower()
  if value == "approve" or value == "request_changes" then
    return value
  end
  return "comment"
end

function PrSubmitEditor.open(opts)
  if active_instance then
    active_instance:cancel_review()
  end
  local instance = PrSubmitEditor(opts)
  active_instance = instance
  return instance:mount()
end

function PrSubmitEditor.is_open()
  return active_instance ~= nil and not active_instance.closed
end

function PrSubmitEditor.get_active()
  return active_instance
end

function PrSubmitEditor.select_outcome(outcome)
  if active_instance then
    active_instance:set_outcome(outcome)
  end
end

function PrSubmitEditor.set_body(text)
  if active_instance then
    active_instance:update_body(text)
  end
end

function PrSubmitEditor.submit()
  if not active_instance then
    return false
  end
  return active_instance:submit_review()
end

function PrSubmitEditor.cancel()
  if active_instance then
    active_instance:cancel_review()
  end
end

return PrSubmitEditor
