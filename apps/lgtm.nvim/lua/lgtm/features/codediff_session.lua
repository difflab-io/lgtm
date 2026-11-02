local Notifications = require("lgtm.ui.notifications")

local CodeDiffSession = {}
local Private = {}

local augroup = vim.api.nvim_create_augroup("LgtmCodeDiffSession", { clear = true })

-- State -----------------------------------------------------------------------

local state = {
  pr_service = nil,
  overlay = nil,
  actions = nil,
  bound_buffers = {},
}

-- Lifecycle -------------------------------------------------------------------

function CodeDiffSession.setup(opts)
  opts = opts or {}
  state.pr_service = opts.pr_service
  state.overlay = opts.overlay
  state.actions = opts.actions

  vim.api.nvim_create_autocmd({ "TabNewEntered", "BufEnter", "WinEnter" }, {
    group = augroup,
    callback = function()
      Private.sync_codediff_tabpage(vim.api.nvim_get_current_tabpage())
    end,
  })
end

-- Methods ---------------------------------------------------------------------

function CodeDiffSession.open_pr(pr)
  assert(state.pr_service, "CodeDiffSession.setup must be called before open_pr")
  local pr_number = type(pr) == "table" and pr.number or pr
  local selected = state.pr_service:select_pr(pr_number)
  if not selected then
    return nil
  end

  if Private.has_git_revisions(selected.base_revision, selected.head_revision) then
    local first_path = selected.files and selected.files[1] and selected.files[1].path or nil
    if
      vim.api.nvim_buf_get_name(0) == ""
      and first_path
      and vim.fn.filereadable(first_path) == 1
    then
      vim.cmd.edit(vim.fn.fnameescape(first_path))
    end
    vim.cmd(string.format("CodeDiff %s %s", selected.base_revision, selected.head_revision))
    vim.wait(600, function()
      local _, ready = Private.sync_codediff_tabpage(vim.api.nvim_get_current_tabpage())
      return ready == true
    end, 20)
    Private.sync_codediff_tabpage(vim.api.nvim_get_current_tabpage())
    if state.overlay then
      state.overlay:refresh_all()
    end
  else
    Private.open_fallback_tabpage(selected)
  end

  Notifications.pr_selected(selected)
  return selected
end

function CodeDiffSession.sync_current_tabpage()
  local synced = Private.sync_codediff_tabpage(vim.api.nvim_get_current_tabpage())
  return synced
end

function CodeDiffSession.reset()
  state.bound_buffers = {}
end

-- Private ---------------------------------------------------------------------

function Private.has_git_revisions(base_rev, head_rev)
  if type(base_rev) ~= "string" or type(head_rev) ~= "string" then
    return false
  end
  if base_rev == "" or head_rev == "" then
    return false
  end
  vim.fn.system({ "git", "rev-parse", "--verify", base_rev .. "^{commit}" })
  if vim.v.shell_error ~= 0 then
    return false
  end
  vim.fn.system({ "git", "rev-parse", "--verify", head_rev .. "^{commit}" })
  return vim.v.shell_error == 0
end

function Private.attach_buffer_keymaps(bufnr)
  if not bufnr or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  if state.bound_buffers[bufnr] or not state.actions then
    return
  end
  state.bound_buffers[bufnr] = true

  local map_opts = {
    buffer = bufnr,
    silent = true,
    nowait = true,
  }
  vim.keymap.set("n", "c", function()
    state.actions.create_or_reply_comment()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Draft PR comment" }))

  vim.keymap.set("n", "x", function()
    state.actions.toggle_resolve_at_cursor()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Toggle thread resolved" }))

  vim.keymap.set("n", "<CR>", function()
    state.actions.open_thread_at_cursor()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Open thread detail" }))

  vim.keymap.set("n", "za", function()
    state.actions.toggle_collapse_at_cursor()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Toggle thread fold" }))

  vim.keymap.set("n", "zA", function()
    state.actions.set_all_collapsed()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Fold/unfold all threads" }))

  vim.keymap.set("n", "]c", function()
    state.actions.jump_next_thread()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Next thread" }))

  vim.keymap.set("n", "[c", function()
    state.actions.jump_prev_thread()
  end, vim.tbl_extend("force", map_opts, { desc = "LGTM: Previous thread" }))
end

function Private.resolve_codediff_relative_path(ref, pr)
  if ref and type(ref.relative) == "string" and ref.relative ~= "" then
    return ref.relative
  end
  if pr and type(pr.files) == "table" and pr.files[1] then
    return pr.files[1].path
  end
  return nil
end

function Private.sync_codediff_tabpage(tabpage)
  if not state.pr_service or not state.overlay then
    return false, false
  end
  local ok, lifecycle = pcall(require, "codediff.ui.lifecycle")
  if not ok or not lifecycle.get_session then
    return false, false
  end
  local session = lifecycle.get_session(tabpage)
  if not session then
    return false, false
  end

  local pr = state.pr_service.state:get_value().selected_pr
  state.pr_service:set_active_tabpage(tabpage)
  local orig_path = Private.resolve_codediff_relative_path(session.original, pr)
  local mod_path = Private.resolve_codediff_relative_path(session.modified, pr) or orig_path

  if session.original_bufnr and vim.api.nvim_buf_is_valid(session.original_bufnr) and orig_path then
    state.overlay:register_diff_buffer(session.original_bufnr, orig_path, "LEFT")
    Private.attach_buffer_keymaps(session.original_bufnr)
  end
  local modified_ready = false
  if session.modified_bufnr and vim.api.nvim_buf_is_valid(session.modified_bufnr) and mod_path then
    state.overlay:register_diff_buffer(session.modified_bufnr, mod_path, "RIGHT")
    Private.attach_buffer_keymaps(session.modified_bufnr)
    modified_ready = vim.api.nvim_buf_line_count(session.modified_bufnr) > 1
  end
  return true, modified_ready
end

function Private.open_fallback_tabpage(pr)
  vim.cmd("tabnew")
  local tabpage = vim.api.nvim_get_current_tabpage()
  state.pr_service:set_active_tabpage(tabpage)

  local explorer_buf = vim.api.nvim_get_current_buf()
  local explorer_win = vim.api.nvim_get_current_win()
  vim.bo[explorer_buf].buftype = "nofile"
  vim.bo[explorer_buf].bufhidden = "wipe"
  vim.bo[explorer_buf].swapfile = false
  vim.bo[explorer_buf].filetype = "codediff-explorer"
  local file_lines = {
    string.format("PR #%d Explorer", pr.number),
    string.rep("─", 24),
  }
  for _, file in ipairs(pr.files or {}) do
    file_lines[#file_lines + 1] = string.format(" %s %s", file.status or "M", file.path)
  end
  vim.api.nvim_buf_set_lines(explorer_buf, 0, -1, false, file_lines)
  vim.bo[explorer_buf].modifiable = false
  vim.api.nvim_win_set_width(explorer_win, 30)

  vim.cmd("rightbelow vsplit")
  local diff_win = vim.api.nvim_get_current_win()
  local first_path = (pr.files and pr.files[1] and pr.files[1].path) or "README.md"
  local target_buf = vim.fn.bufadd(vim.fn.fnamemodify(first_path, ":p"))
  vim.fn.bufload(target_buf)
  vim.api.nvim_win_set_buf(diff_win, target_buf)

  state.overlay:register_diff_buffer(target_buf, first_path, "RIGHT")
  Private.attach_buffer_keymaps(target_buf)
  return tabpage
end

return CodeDiffSession
