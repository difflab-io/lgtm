local n = require("nui-components")
local Thread = require("lgtm.models.thread")

local CommentEditor = {}
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
    UI.editor_input(session),
    UI.error_banner(session),
    UI.footer_buttons(session)
  )
end

-- UI --------------------------------------------------------------------------

function UI.header(session)
  return n.paragraph({
    lines = session.title_text,
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

function UI.editor_input(session)
  return n.text_input({
    id = "comment_body_input",
    flex = 1,
    autofocus = true,
    wrap = true,
    border_label = {
      text = session.input_label,
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

function UI.footer_buttons(session)
  return n.columns(
    {
      size = 3,
      flex = 0,
    },
    n.gap({
      flex = 1,
    }),
    n.button({
      id = "comment_submit_btn",
      label = " [ Submit (<C-s>) ] ",
      on_press = function()
        CommentEditor.submit()
      end,
    }),
    n.gap(2),
    n.button({
      id = "comment_cancel_btn",
      label = " [ Cancel (<Esc>) ] ",
      on_press = function()
        CommentEditor.cancel()
      end,
    }),
    n.gap(1)
  )
end

-- Lifecycle -------------------------------------------------------------------

---Open the floating comment editor for creating, replying to, or editing a comment.
---@param opts { action: "create"|"reply"|"edit", mode?: "local"|"pr", path?: string, side?: "LEFT"|"RIGHT", start_line?: integer, line?: integer, thread?: LgtmThread, comment?: LgtmComment, initial_body?: string, on_submit: fun(body: string), on_cancel?: fun() }
---@return table
function CommentEditor.open(opts)
  assert(type(opts) == "table", "CommentEditor.open requires options")
  if active_session then
    CommentEditor.cancel()
  end

  local initial_body = opts.initial_body or (opts.comment and opts.comment.body) or ""
  local signal = n.create_signal({
    body = initial_body,
    error = "",
  })

  local renderer = n.create_renderer({
    width = 76,
    height = 14,
  })

  local session = {
    action = opts.action or "create",
    mode = opts.mode or "local",
    path = opts.path or (opts.thread and opts.thread.path) or "buffer",
    side = opts.side or (opts.thread and opts.thread.side) or "RIGHT",
    start_line = opts.start_line or (opts.thread and opts.thread.start_line) or opts.line or 1,
    line = opts.line or (opts.thread and opts.thread.line) or 1,
    thread = opts.thread,
    comment = opts.comment,
    title_text = Private.format_header_title(opts),
    input_label = Private.format_input_label(opts),
    signal = signal,
    renderer = renderer,
    origin_win = vim.api.nvim_get_current_win(),
    on_submit = assert(opts.on_submit, "CommentEditor.open requires on_submit"),
    on_cancel = opts.on_cancel,
    closed = false,
  }

  renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        CommentEditor.submit()
      end,
    },
    {
      mode = { "n" },
      key = "<Esc>",
      handler = function()
        CommentEditor.cancel()
      end,
    },
    {
      mode = { "n" },
      key = "q",
      handler = function()
        CommentEditor.cancel()
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

---Return true when the comment editor modal is currently mounted.
---@return boolean
function CommentEditor.is_open()
  return active_session ~= nil and not active_session.closed
end

---Return the active comment editor session table, or nil if closed.
---@return table|nil
function CommentEditor.get_active()
  return active_session
end

---Programmatically update the comment input buffer and reactive signal value.
---@param text string
function CommentEditor.set_text(text)
  if not active_session then
    return
  end
  active_session.signal.body = text
  local input_cmp = active_session.renderer:get_component_by_id("comment_body_input")
  if input_cmp and input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
    local lines = vim.split(text or "", "\n", { plain = true })
    vim.api.nvim_buf_set_lines(input_cmp.bufnr, 0, -1, false, lines)
  end
end

---Validate and submit the current comment editor content.
---@return boolean
function CommentEditor.submit()
  if not active_session or active_session.closed then
    return false
  end
  local session = active_session
  local body = Private.read_current_body(session)
  if body == "" then
    session.signal.error = "Comment body cannot be empty."
    return false
  end
  Private.close_session(session)
  session.on_submit(body)
  return true
end

---Cancel and close the active comment editor without saving.
function CommentEditor.cancel()
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
  local input_cmp = session.renderer:get_component_by_id("comment_body_input")
  if input_cmp and input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
    local lines = vim.api.nvim_buf_get_lines(input_cmp.bufnr, 0, -1, false)
    local from_buf = vim.trim(table.concat(lines, "\n"))
    if from_buf ~= "" then
      return from_buf
    end
  end
  local body = session.signal:get_value().body
  return type(body) == "string" and vim.trim(body) or ""
end

function Private.format_header_title(opts)
  local mode_badge = (opts.mode == "pr") and "[PR Review]" or "[Local Review]"
  if opts.action == "reply" and opts.thread then
    return string.format("%s Reply to thread at %s", mode_badge, Thread.format_anchor(opts.thread))
  elseif opts.action == "edit" and opts.thread then
    return string.format("%s Edit comment at %s", mode_badge, Thread.format_anchor(opts.thread))
  end
  local path = opts.path or "buffer"
  local line = opts.line or 1
  local start_line = opts.start_line or line
  local anchor = start_line < line and string.format("%s:%d-%d", path, start_line, line)
    or string.format("%s:%d", path, line)
  return string.format("%s New comment at %s", mode_badge, anchor)
end

function Private.format_input_label(opts)
  if opts.action == "reply" then
    return " Reply Body "
  elseif opts.action == "edit" then
    return " Edit Comment "
  end
  return " Comment Body "
end

return CommentEditor
