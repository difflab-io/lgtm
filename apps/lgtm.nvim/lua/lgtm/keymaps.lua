local Keymaps = {}
local Private = {}

-- Methods ---------------------------------------------------------------------

function Keymaps.setup(config, actions)
  local keys = (config and config.keymaps) or {}
  local specs = Private.build_specs(keys, actions)
  local ok, wk = pcall(require, "which-key")
  if ok and type(wk.add) == "function" then
    wk.add(specs)
  end

  for _, spec in ipairs(specs) do
    if type(spec[2]) == "function" then
      vim.keymap.set(spec.mode or "n", spec[1], spec[2], {
        silent = true,
        desc = spec.desc,
      })
    end
  end

  Private.attach_normal_buffer_cr(vim.api.nvim_get_current_buf(), actions)
  vim.api.nvim_create_autocmd("BufEnter", {
    group = vim.api.nvim_create_augroup("LgtmBufferKeymaps", { clear = true }),
    callback = function(ev)
      Private.attach_normal_buffer_cr(ev.buf, actions)
    end,
  })
end

-- Private ---------------------------------------------------------------------

function Private.attach_normal_buffer_cr(bufnr, actions)
  if not vim.api.nvim_buf_is_valid(bufnr) or vim.bo[bufnr].buftype ~= "" then
    return
  end
  vim.keymap.set("n", "<CR>", function()
    if not actions.open_thread_at_cursor({ quiet = true }) then
      vim.api.nvim_feedkeys(vim.api.nvim_replace_termcodes("<CR>", true, false, true), "n", false)
    end
  end, {
    buffer = bufnr,
    silent = true,
    desc = "LGTM: Open thread detail at cursor",
  })
end

function Private.build_specs(keys, actions)
  return {
    {
      keys.prefix,
      group = "LGTM Review",
    },
    {
      keys.comment,
      function()
        actions.create_or_reply_comment()
      end,
      desc = "LGTM: Draft comment or reply",
      mode = { "n", "v" },
    },
    {
      keys.pr_select,
      function()
        actions.open_pr_select()
      end,
      desc = "LGTM: Select open pull request",
      mode = "n",
    },
    {
      keys.threads_toggle,
      function()
        actions.toggle_threads_pane()
      end,
      desc = "LGTM: Toggle threads pane",
      mode = "n",
    },
    {
      keys.thread_open,
      function()
        actions.open_thread_at_cursor()
      end,
      desc = "LGTM: Open thread detail at cursor",
      mode = "n",
    },
    {
      keys.thread_resolve,
      function()
        actions.toggle_resolve_at_cursor()
      end,
      desc = "LGTM: Toggle thread resolved",
      mode = "n",
    },
    {
      keys.pr_submit,
      function()
        actions.open_pr_submit("COMMENT")
      end,
      desc = "LGTM: Submit PR review",
      mode = "n",
    },
    {
      keys.pr_approve,
      function()
        actions.open_pr_submit("APPROVE")
      end,
      desc = "LGTM: Approve pull request",
      mode = "n",
    },
    {
      keys.pr_request_changes,
      function()
        actions.open_pr_submit("REQUEST_CHANGES")
      end,
      desc = "LGTM: Request PR changes",
      mode = "n",
    },
    {
      keys.next_thread,
      function()
        actions.jump_next_thread()
      end,
      desc = "LGTM: Next review thread",
      mode = "n",
    },
    {
      keys.prev_thread,
      function()
        actions.jump_prev_thread()
      end,
      desc = "LGTM: Previous review thread",
      mode = "n",
    },
  }
end

return Keymaps
