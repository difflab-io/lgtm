local n = require("nui-components")
local ReviewSubmission = require("lgtm.models.review_submission")

local PrSubmitEditor = {}
local UI = {}
local Private = {}

-- State -----------------------------------------------------------------------

local active_session = nil

-- Render ----------------------------------------------------------------------

function UI.layout(session)
  return n.rows(
    {
      flex = 0,
    },
    UI.header(session),
    UI.outcome_selector(session),
    UI.body_input(session),
    UI.error_banner(session),
    UI.footer_buttons(session)
  )
end

-- UI --------------------------------------------------------------------------

function UI.header(session)
  return n.paragraph({
    lines = string.format(
      "Submit Review for PR #%d · %s (@%s)",
      session.pr.number,
      session.pr.title,
      session.pr.author
    ),
    align = "left",
    is_focusable = false,
    padding = {
      top = 0,
      bottom = 0,
      left = 1,
      right = 1,
    },
  })
end

function UI.outcome_selector(session)
  return n.select({
    id = "pr_submit_outcome_select",
    size = 5,
    border_label = {
      text = " Review Outcome ",
      align = "left",
    },
    selected = session.signal.selected_outcome,
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
      session.signal.selected_outcome = node
      session.signal.outcome = node.id
      session.signal.error = ""
    end,
  })
end

function UI.body_input(session)
  return n.text_input({
    id = "pr_submit_body_input",
    flex = 1,
    autofocus = true,
    wrap = true,
    border_label = {
      text = " Overall Review Summary ",
      align = "left",
    },
    value = session.signal.body,
    on_change = function(value)
      session.signal.body = value
      session.signal.error = ""
    end,
  })
end

function UI.error_banner(session)
  local has_error = session.signal.error:dup():map(function(err)
    return type(err) == "string" and err ~= ""
  end)
  return n.paragraph({
    lines = session.signal.error,
    hidden = has_error:negate(),
    is_focusable = false,
    padding = {
      top = 0,
      bottom = 0,
      left = 1,
      right = 1,
    },
  })
end

function UI.footer_buttons(_session)
  return n.columns(
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
        PrSubmitEditor.submit()
      end,
    }),
    n.gap(2),
    n.button({
      id = "pr_cancel_btn",
      label = " [ Cancel (<Esc>) ] ",
      on_press = function()
        PrSubmitEditor.cancel()
      end,
    }),
    n.gap(1)
  )
end

-- Lifecycle -------------------------------------------------------------------

---Open the floating PR review submission editor with outcome selector and Submit/Cancel buttons.
---@param opts { pr: LgtmPullRequest, initial_outcome?: "comment"|"approve"|"request_changes", initial_body?: string, on_submit: fun(payload: { outcome: string, body: string }), on_cancel?: fun() }
---@return table
function PrSubmitEditor.open(opts)
  assert(type(opts) == "table" and opts.pr, "PrSubmitEditor.open requires an active PR")
  if active_session then
    PrSubmitEditor.cancel()
  end

  local initial_outcome = Private.normalize_outcome(opts.initial_outcome)
  local initial_body = opts.initial_body or ""
  local signal = n.create_signal({
    outcome = initial_outcome,
    selected_outcome = {
      id = initial_outcome,
      text = ReviewSubmission.outcome_label(initial_outcome),
    },
    body = initial_body,
    error = "",
  })

  local renderer = n.create_renderer({
    width = 80,
    height = 18,
  })

  local session = {
    pr = opts.pr,
    signal = signal,
    renderer = renderer,
    origin_win = vim.api.nvim_get_current_win(),
    on_submit = assert(opts.on_submit, "PrSubmitEditor.open requires on_submit"),
    on_cancel = opts.on_cancel,
    closed = false,
  }

  renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        PrSubmitEditor.submit()
      end,
    },
    {
      mode = { "n" },
      key = "<Esc>",
      handler = function()
        PrSubmitEditor.cancel()
      end,
    },
    {
      mode = { "n" },
      key = "q",
      handler = function()
        PrSubmitEditor.cancel()
      end,
    },
  })

  renderer:on_unmount(function()
    if active_session == session then
      active_session = nil
    end
  end)

  active_session = session
  renderer:render(function()
    return UI.layout(session)
  end)
  return session
end

-- Methods ---------------------------------------------------------------------

---Return true when the PR submit editor modal is mounted.
---@return boolean
function PrSubmitEditor.is_open()
  return active_session ~= nil and not active_session.closed
end

---Return the active PR submit editor session table, or nil.
---@return table|nil
function PrSubmitEditor.get_active()
  return active_session
end

---Programmatically select the review outcome (`comment`, `approve`, `request_changes`).
---@param outcome "comment"|"approve"|"request_changes"
function PrSubmitEditor.select_outcome(outcome)
  if not active_session then
    return
  end
  local normalized = Private.normalize_outcome(outcome)
  active_session.signal.outcome = normalized
  active_session.signal.selected_outcome = {
    id = normalized,
    text = ReviewSubmission.outcome_label(normalized),
  }
end

---Programmatically update the review summary body.
---@param text string
function PrSubmitEditor.set_body(text)
  if not active_session then
    return
  end
  active_session.signal.body = text
  local input_cmp = active_session.renderer:get_component_by_id("pr_submit_body_input")
  if input_cmp and input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
    local lines = vim.split(text or "", "\n", { plain = true })
    vim.api.nvim_buf_set_lines(input_cmp.bufnr, 0, -1, false, lines)
  end
end

---Validate and submit the overall PR review.
---@return boolean
function PrSubmitEditor.submit()
  if not active_session or active_session.closed then
    return false
  end
  local session = active_session
  local snapshot = session.signal:get_value()
  local outcome = snapshot.outcome or "comment"
  local body = Private.read_current_body(session)

  if (outcome == "comment" or outcome == "request_changes") and body == "" then
    session.signal.error = string.format(
      "Review summary body is required when outcome is %s.",
      ReviewSubmission.outcome_label(outcome)
    )
    return false
  end

  Private.close_session(session)
  session.on_submit({
    outcome = outcome,
    body = body,
  })
  return true
end

---Cancel and close the PR submit editor without submitting.
function PrSubmitEditor.cancel()
  if not active_session or active_session.closed then
    return
  end
  local session = active_session
  Private.close_session(session)
  if session.on_cancel then
    session.on_cancel()
  end
end

-- Private ---------------------------------------------------------------------

function Private.close_session(session)
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
  if session.origin_win and vim.api.nvim_win_is_valid(session.origin_win) then
    pcall(vim.api.nvim_set_current_win, session.origin_win)
  end
  if not was_mounted then
    vim.schedule(function()
      pcall(vim.cmd, "stopinsert")
      pcall(function()
        session.renderer:close()
      end)
      local cur_cfg = vim.api.nvim_win_get_config(0)
      if
        cur_cfg.relative ~= ""
        and session.origin_win
        and vim.api.nvim_win_is_valid(session.origin_win)
      then
        pcall(vim.api.nvim_set_current_win, session.origin_win)
      end
    end)
  end
end

function Private.read_current_body(session)
  local input_cmp = session.renderer:get_component_by_id("pr_submit_body_input")
  if input_cmp and input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
    local lines = vim.api.nvim_buf_get_lines(input_cmp.bufnr, 0, -1, false)
    local from_buf = Private.trim(table.concat(lines, "\n"))
    if from_buf ~= "" then
      return from_buf
    end
  end
  return Private.trim(session.signal:get_value().body)
end

function Private.normalize_outcome(outcome)
  local value = Private.trim(outcome):lower()
  if value == "approve" or value == "request_changes" then
    return value
  end
  return "comment"
end

function Private.trim(value)
  if type(value) ~= "string" then
    return ""
  end
  return (value:gsub("^%s+", ""):gsub("%s+$", ""))
end

return PrSubmitEditor
