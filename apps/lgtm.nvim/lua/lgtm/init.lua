local App = require("lgtm.app")

local M = {}

-- Lifecycle -------------------------------------------------------------------

---Configure LGTM UI scaffolding and load required dependencies.
function M.setup(opts)
  require("codediff")
  require("which-key")
  require("notify")
  require("nui-components")
  require("nui.popup")
  pcall(require, "kak.keybinds")

  return App.setup(opts)
end

function M.reset(opts)
  return App.reset(opts)
end

-- Methods ---------------------------------------------------------------------

function M.get_mode()
  return App.get_mode()
end

function M.get_services()
  return App.get_services()
end

function M.open_pr_select()
  return App.open_pr_select()
end

function M.toggle_threads_pane()
  return App.toggle_threads_pane()
end

function M.open_thread_at_cursor(opts)
  return App.open_thread_at_cursor(opts)
end

function M.create_or_reply_comment()
  return App.create_or_reply_comment()
end

function M.toggle_resolve_at_cursor()
  return App.toggle_resolve_at_cursor()
end

function M.open_pr_submit(outcome)
  return App.open_pr_submit(outcome)
end

function M.jump_next_thread()
  return App.jump_next_thread()
end

function M.jump_prev_thread()
  return App.jump_prev_thread()
end

return M
