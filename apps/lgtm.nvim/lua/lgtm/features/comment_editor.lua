local Object = require("nui.object")
local Thread = require("lgtm.models.thread")
local n = require("nui-components")

local CommentEditor = Object("CommentEditor")

local active_instance = nil

function CommentEditor:init(opts)
  assert(type(opts) == "table", "CommentEditor requires options")
  local initial_body = opts.initial_body or (opts.comment and opts.comment.body) or ""

  self.action = opts.action or "create"
  self.mode = opts.mode or "local"
  self.path = opts.path or (opts.thread and opts.thread.path) or "buffer"
  self.side = opts.side or (opts.thread and opts.thread.side) or "RIGHT"
  self.start_line = opts.start_line or (opts.thread and opts.thread.start_line) or opts.line or 1
  self.line = opts.line or (opts.thread and opts.thread.line) or 1
  self.thread = opts.thread
  self.comment = opts.comment
  self.title_text = self:_format_header_title(opts)
  self.input_label = self:_format_input_label(opts)
  self.on_submit = assert(opts.on_submit, "CommentEditor requires on_submit")
  self.on_cancel = opts.on_cancel
  self.origin_win = vim.api.nvim_get_current_win()
  self.closed = false

  self.signal = n.create_signal({
    body = initial_body,
    error = "",
  })

  self.renderer = n.create_renderer({
    width = 76,
    height = 14,
    on_unmount = function()
      if active_instance == self then
        active_instance = nil
      end
    end,
  })
end

function CommentEditor:mount()
  self.renderer:add_mappings({
    {
      mode = { "n", "i" },
      key = "<C-s>",
      handler = function()
        self:submit_comment()
      end,
    },
    {
      mode = { "n" },
      key = { "<Esc>", "q" },
      handler = function()
        self:cancel_comment()
      end,
    },
  })

  self.renderer:render(function()
    return self:render_body()
  end)

  return self
end

function CommentEditor:render_body()
  local has_error = self.signal.error:dup():map(function(err)
    return type(err) == "string" and err ~= ""
  end)

  return n.form(
    {
      id = "comment_editor_form",
      submit_key = "<C-s>",
      on_submit = function(is_valid)
        if is_valid then
          self:submit_comment()
        end
      end,
    },
    n.paragraph({
      lines = self.title_text,
      align = "left",
      is_focusable = false,
      padding = {
        top = 0,
        bottom = 0,
        left = 1,
        right = 1,
      },
    }),
    n.text_input({
      id = "comment_body_input",
      flex = 1,
      autofocus = true,
      wrap = true,
      border_label = {
        text = self.input_label,
        align = "left",
      },
      value = self.signal.body,
      validate = function(value)
        return type(value) == "string" and vim.trim(value) ~= ""
      end,
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
        id = "comment_submit_btn",
        label = " [ Submit (<C-s>) ] ",
        on_press = function()
          self:submit_comment()
        end,
      }),
      n.gap(2),
      n.button({
        id = "comment_cancel_btn",
        label = " [ Cancel (<Esc>) ] ",
        on_press = function()
          self:cancel_comment()
        end,
      }),
      n.gap(1)
    )
  )
end

function CommentEditor:update_text(text)
  self.signal.body = text
  local input_cmp = self.renderer:get_component_by_id("comment_body_input")
  if input_cmp then
    input_cmp:set_current_value(text)
    if input_cmp.bufnr and vim.api.nvim_buf_is_valid(input_cmp.bufnr) then
      local lines = vim.split(text or "", "\n", { plain = true })
      vim.api.nvim_buf_set_lines(input_cmp.bufnr, 0, -1, false, lines)
    end
  end
end

function CommentEditor:submit_comment()
  if self.closed then
    return false
  end
  local body = self:_read_current_body()
  if body == "" then
    self.signal.error = "Comment body cannot be empty."
    return false
  end
  self:_close_window()
  self.on_submit(body)
  return true
end

function CommentEditor:cancel_comment()
  if self.closed then
    return
  end
  self:_close_window()
  if self.on_cancel then
    self.on_cancel()
  end
end

function CommentEditor:_read_current_body()
  local input_cmp = self.renderer:get_component_by_id("comment_body_input")
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

function CommentEditor:_close_window()
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

function CommentEditor:_format_header_title(opts)
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

function CommentEditor:_format_input_label(opts)
  if opts.action == "reply" then
    return " Reply Body "
  elseif opts.action == "edit" then
    return " Edit Comment "
  end
  return " Comment Body "
end

function CommentEditor.open(opts)
  if active_instance then
    active_instance:cancel_comment()
  end
  local instance = CommentEditor(opts)
  active_instance = instance
  return instance:mount()
end

function CommentEditor.is_open()
  return active_instance ~= nil and not active_instance.closed
end

function CommentEditor.get_active()
  return active_instance
end

function CommentEditor.set_text(text)
  if active_instance then
    active_instance:update_text(text)
  end
end

function CommentEditor.submit()
  if not active_instance then
    return false
  end
  return active_instance:submit_comment()
end

function CommentEditor.cancel()
  if active_instance then
    active_instance:cancel_comment()
  end
end

return CommentEditor
