local CodeDiffSession = require("lgtm.features.codediff_session")
local CommentEditor = require("lgtm.features.comment_editor")
local Config = require("lgtm.config")
local Keymaps = require("lgtm.keymaps")
local LocalReviewService = require("lgtm.services.local_review_service")
local Notifications = require("lgtm.ui.notifications")
local PrSelectModal = require("lgtm.features.pr_select_modal")
local PrService = require("lgtm.services.pr_service")
local PrSubmitEditor = require("lgtm.features.pr_submit_editor")
local ThreadsOverlay = require("lgtm.ui.threads_overlay")
local ThreadsPane = require("lgtm.features.threads_pane")

local App = {}
local Private = {}

-- State -----------------------------------------------------------------------

local state = {
  config = nil,
  local_service = nil,
  pr_service = nil,
  overlay = nil,
  initialized = false,
}

-- Lifecycle -------------------------------------------------------------------

function App.setup(opts)
  state.config = Config.setup(opts)
  state.local_service = LocalReviewService.new()
  state.pr_service = PrService.new({
    seed_prs = state.config.seed_prs,
  })
  if state.overlay then
    state.overlay:detach()
  end
  state.overlay = ThreadsOverlay.new({
    local_service = state.local_service,
    pr_service = state.pr_service,
  })
  state.overlay:attach()

  CodeDiffSession.setup({
    pr_service = state.pr_service,
    overlay = state.overlay,
    actions = App,
  })
  Keymaps.setup(state.config, App)
  Private.register_user_command()
  state.initialized = true
  return App
end

function App.reset(opts)
  CommentEditor.cancel()
  PrSubmitEditor.cancel()
  PrSelectModal.reset()
  ThreadsPane.close()
  CodeDiffSession.reset()
  Notifications.clear()
  return App.setup(opts)
end

-- Methods ---------------------------------------------------------------------

function App.get_mode()
  Private.ensure_initialized()
  local tabpage = vim.api.nvim_get_current_tabpage()
  if state.pr_service:is_pr_tabpage(tabpage) then
    return "pr"
  end
  return "local"
end

function App.get_services()
  Private.ensure_initialized()
  return {
    local_service = state.local_service,
    pr_service = state.pr_service,
    overlay = state.overlay,
  }
end

function App.open_pr_select()
  Private.ensure_initialized()
  return PrSelectModal.open({
    pr_service = state.pr_service,
    on_select = function(pr)
      CodeDiffSession.open_pr(pr)
      Private.refresh_ui()
    end,
  })
end

function App.toggle_threads_pane()
  Private.ensure_initialized()
  local mode = App.get_mode()
  return ThreadsPane.toggle({
    mode = mode,
    service = Private.active_service(mode),
    on_jump = Private.jump_to_thread_location,
  })
end

function App.open_thread_at_cursor(opts)
  opts = opts or {}
  Private.ensure_initialized()
  local target = Private.resolve_cursor_target()
  local thread = Private.get_thread_at_target(target)
  if not thread then
    if not opts.quiet then
      Notifications.notify("No review thread anchored on current line", "warn")
    end
    return nil
  end
  local mode = App.get_mode()
  Notifications.thread_opened(thread)
  ThreadsPane.open({
    mode = mode,
    service = Private.active_service(mode),
    view = "detail",
    thread_id = thread.id,
    on_jump = Private.jump_to_thread_location,
  })
  return thread
end

function App.create_or_reply_comment()
  Private.ensure_initialized()
  local mode = App.get_mode()
  local target = Private.resolve_cursor_target()
  if not target.path or target.path == "" then
    Notifications.notify("Open a file or PR diff buffer before drafting a comment", "warn")
    return nil
  end
  local existing = Private.get_thread_at_target(target)
  if existing then
    return CommentEditor.open({
      action = "reply",
      mode = mode,
      thread = existing,
      on_submit = function(body)
        Private.reply_thread_from_editor(mode, existing, body)
      end,
    })
  end

  return CommentEditor.open({
    action = "create",
    mode = mode,
    path = target.path,
    side = target.side,
    line = target.line,
    on_submit = function(body)
      Private.create_thread_from_editor(mode, target, body)
    end,
  })
end

function App.toggle_resolve_at_cursor()
  Private.ensure_initialized()
  local target = Private.resolve_cursor_target()
  local thread = Private.get_thread_at_target(target)
  if not thread then
    Notifications.notify("No review thread anchored on current line", "warn")
    return nil
  end
  local service = Private.active_service()
  local updated = service:toggle_resolved(thread.id)
  if updated then
    Notifications.thread_resolved(updated)
    Private.refresh_ui()
  end
  return updated
end

function App.toggle_collapse_at_cursor()
  Private.ensure_initialized()
  local target = Private.resolve_cursor_target()
  local thread = Private.get_thread_at_target(target)
  if not thread then
    return nil
  end
  local service = Private.active_service()
  local updated = service:toggle_collapsed(thread.id)
  Private.refresh_ui()
  return updated
end

function App.set_all_collapsed(collapsed)
  Private.ensure_initialized()
  local service = Private.active_service()
  local threads = service:get_threads()
  local next_collapsed = collapsed
  if next_collapsed == nil then
    next_collapsed = false
    for _, thread in ipairs(threads) do
      if not thread.collapsed then
        next_collapsed = true
        break
      end
    end
  end
  service:set_all_collapsed(next_collapsed)
  Private.refresh_ui()
  return next_collapsed
end

function App.open_pr_submit(default_outcome)
  Private.ensure_initialized()
  local pr = state.pr_service.state:get_value().selected_pr
  if not pr then
    Notifications.notify("Select an open pull request first (<leader>rp)", "warn")
    return nil
  end
  return PrSubmitEditor.open({
    pr = pr,
    initial_outcome = (default_outcome or "comment"):lower(),
    on_submit = function(payload)
      local submission = state.pr_service:submit_review({
        outcome = payload.outcome,
        body = payload.body,
      })
      Notifications.review_submitted(pr, submission)
      Private.refresh_ui()
    end,
  })
end

function App.jump_next_thread()
  Private.ensure_initialized()
  return state.overlay:jump_thread(1)
end

function App.jump_prev_thread()
  Private.ensure_initialized()
  return state.overlay:jump_thread(-1)
end

function App.dispatch_command(fargs)
  Private.ensure_initialized()
  local subcmd = (fargs and fargs[1]) or "threads"
  local handlers = {
    pr = function()
      App.open_pr_select()
    end,
    threads = function()
      App.toggle_threads_pane()
    end,
    comment = function()
      App.create_or_reply_comment()
    end,
    thread = function()
      App.open_thread_at_cursor()
    end,
    resolve = function()
      App.toggle_resolve_at_cursor()
    end,
    submit = function()
      App.open_pr_submit("comment")
    end,
    approve = function()
      App.open_pr_submit("approve")
    end,
    request_changes = function()
      App.open_pr_submit("request_changes")
    end,
  }
  local handler = handlers[subcmd]
  if handler then
    handler()
  else
    Notifications.notify(string.format("Unknown :Lgtm subcommand '%s'", tostring(subcmd)), "warn")
  end
end

-- Private ---------------------------------------------------------------------

function Private.ensure_initialized()
  if not state.initialized then
    App.setup()
  end
end

function Private.active_service(mode)
  local resolved_mode = mode or App.get_mode()
  if resolved_mode == "pr" then
    return state.pr_service
  end
  return state.local_service
end

function Private.resolve_cursor_target()
  local bufnr = vim.api.nvim_get_current_buf()
  local winid = vim.api.nvim_get_current_win()
  local line = vim.api.nvim_win_get_cursor(winid)[1]
  local path, side = state.overlay:get_buffer_target(bufnr)
  return {
    bufnr = bufnr,
    winid = winid,
    line = line,
    path = path,
    side = side,
  }
end

function Private.get_thread_at_target(target)
  local from_overlay = state.overlay:get_thread_at_cursor(target.winid)
  if from_overlay then
    return from_overlay
  end
  if not target.path or target.path == "" then
    return nil
  end
  if App.get_mode() == "pr" then
    return state.pr_service:get_thread_at_line(target.path, target.line, target.side)
  end
  return state.local_service:get_thread_at_line(target.path, target.line)
end

function Private.jump_to_thread_location(thread)
  if not thread or not thread.path then
    return
  end
  for _, winid in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    local bufnr = vim.api.nvim_win_get_buf(winid)
    local path = state.overlay:get_buffer_target(bufnr)
    if path == thread.path then
      vim.api.nvim_set_current_win(winid)
      local max_line = math.max(1, vim.api.nvim_buf_line_count(bufnr))
      vim.api.nvim_win_set_cursor(winid, { math.min(thread.line, max_line), 0 })
      return
    end
  end
end

function Private.refresh_ui()
  if state.overlay then
    state.overlay:refresh_all()
  end
end

function Private.create_thread_from_editor(mode, target, body)
  local service = Private.active_service(mode)
  local thread = service:create_thread({
    path = target.path,
    line = target.line,
    side = target.side,
    body = body,
  })
  Notifications.thread_created(thread)
  Private.refresh_ui()
  return thread
end

function Private.reply_thread_from_editor(mode, thread, body)
  local service = Private.active_service(mode)
  local updated = service:reply_to_thread(thread.id, body)
  if updated then
    Notifications.reply_added(updated)
    Private.refresh_ui()
  end
  return updated
end

function Private.register_user_command()
  pcall(vim.api.nvim_del_user_command, "Lgtm")
  vim.api.nvim_create_user_command("Lgtm", function(cmd_opts)
    App.dispatch_command(cmd_opts.fargs)
  end, {
    nargs = "*",
    desc = "LGTM review commands",
    complete = function()
      return {
        "pr",
        "threads",
        "comment",
        "thread",
        "resolve",
        "submit",
        "approve",
        "request_changes",
      }
    end,
  })
end

return App
