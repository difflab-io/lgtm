-- State -----------------------------------------------------------------------

local M = {}
local Private = {}

-- Methods ---------------------------------------------------------------------

function M.setup(opts)
  require("lgtm.config").setup(opts)
  require("lgtm.ui.theme").highlights()
  if vim.fn.has("nvim-0.11") == 0 then
    error("lgtm.nvim requires Neovim >= 0.11")
  end
  if pcall(require, "which-key") then
    require("lgtm.keymaps").setup()
  end
  local group = vim.api.nvim_create_augroup("Lgtm", { clear = true })
  vim.api.nvim_create_autocmd("TabEnter", {
    group = group,
    callback = function()
      local contexts = require("lgtm.services.state")
      local state = contexts.tabs[vim.api.nvim_get_current_tabpage()]
      if state then
        contexts.set(state)
        -- CodeDiff resumes its own mappings in a scheduled callback first.
        vim.schedule(function()
          vim.schedule(function()
            if
              not state.closed
              and state.tab == vim.api.nvim_get_current_tabpage()
              and state.wait_ready
            then
              state.wait_ready()
              if state.kind == "directory" then
                require("lgtm.app").refresh()
              end
            end
          end)
        end)
      end
    end,
  })
  vim.api.nvim_create_autocmd("TabLeave", {
    group = group,
    callback = function()
      local state = require("lgtm.services.state").tabs[vim.api.nvim_get_current_tabpage()]
      if state then
        require("lgtm.features.diff_view").suspend(state)
      end
    end,
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = "CodeDiffClose",
    callback = function(event)
      local state = require("lgtm.services.state").tabs[event.data.tabpage]
      if state then
        require("lgtm.features.diff_view").release(state)
      end
    end,
  })
  vim.api.nvim_create_autocmd("TabClosed", {
    group = group,
    callback = function()
      for tab, state in pairs(require("lgtm.services.state").tabs) do
        if not vim.api.nvim_tabpage_is_valid(tab) then
          require("lgtm.features.diff_view").release(state)
          local contexts = require("lgtm.services.state")
          if contexts.current == state then
            contexts.set(contexts.tabs[vim.api.nvim_get_current_tabpage()])
          end
        end
      end
    end,
  })
  vim.api.nvim_create_autocmd("BufEnter", {
    group = group,
    callback = function(event)
      require("lgtm.app").restore_notes(event.buf)
    end,
  })
  vim.api.nvim_create_autocmd("BufFilePost", {
    group = group,
    callback = function(event)
      if
        require("lgtm.services.state").notes[event.buf]
        and not require("lgtm.app").owns_review_buffer(event.buf)
      then
        vim.schedule(Private.refresh_sources)
      end
    end,
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = { "MiniFilesActionRename", "LgtmPathRenamed" },
    callback = function()
      vim.schedule(Private.refresh_sources)
    end,
  })
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufWritePost" }, {
    group = group,
    callback = function(event)
      local notes = require("lgtm.app")
      local state = require("lgtm.services.state").notes[event.buf]
      if not state or notes.owns_review_buffer(event.buf) then
        return
      end
      if event.event == "BufWritePost" then
        state.refresh()
      else
        require("lgtm.services.notes_service").invalidate(state)
        vim.api.nvim_buf_clear_namespace(
          event.buf,
          require("lgtm.features.annotation_overlay").namespace,
          0,
          -1
        )
      end
    end,
  })
  vim.api.nvim_create_autocmd({ "WinResized", "ColorScheme" }, {
    group = group,
    callback = function()
      require("lgtm.ui.theme").highlights()
      vim.schedule(function()
        local state = require("lgtm.services.state").current
        if state and state.render_ready and state.render then
          state.render()
        end
        for buf, notes in pairs(require("lgtm.services.state").notes) do
          if
            vim.api.nvim_buf_is_valid(buf)
            and #vim.fn.win_findbuf(buf) > 0
            and notes.conversation
            and not require("lgtm.app").owns_review_buffer(buf)
          then
            require("lgtm.features.annotation_overlay").render(notes, buf, notes.path, "new")
          end
        end
      end)
    end,
  })
  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = group,
    callback = function()
      require("lgtm.services.connect").cancel_all()
      local state = require("lgtm.services.state").current
      if state and state.temp_dir then
        vim.fn.delete(state.temp_dir, "rf")
      end
    end,
  })
end

function M.command(args, range)
  local action, rest = args:match("^(%S*)%s*(.-)$")
  local review = require("lgtm.app")
  local commands = {
    review = require("lgtm.features.new_review_dialog").open,
    directory = function()
      require("lgtm.features.new_review_dialog").directory(rest ~= "" and rest or nil)
    end,
    prs = function()
      require("lgtm.features.branch_pr_selector").open(rest ~= "" and rest or nil)
    end,
    sessions = function()
      require("lgtm.features.session_selector").select(rest == "completed")
    end,
    note = function()
      require("lgtm.app").add_note(range)
    end,
    notes = function()
      require("lgtm.app").show_notes()
    end,
    promote = function()
      require("lgtm.app").promote_note()
    end,
    comment = function()
      review.comment(rest ~= "" and rest or "line", range)
    end,
    threads = require("lgtm.features.threads_pane").open,
    files = review.files,
    save = review.save,
    refresh = review.refresh,
    close = review.close,
    complete = review.complete,
    layout = function()
      require("lgtm.features.diff_view").toggle(require("lgtm.services.state").current)
    end,
    submit = review.submit,
    reconcile = review.reconcile,
    sync = review.sync,
    export = function()
      require("lgtm.app").export(rest ~= "" and rest or "review")
    end,
  }
  if commands[action] then
    commands[action]()
  else
    require("lgtm.ui.notifications").show(
      "Lgtm: review | directory [path] | prs [URL] | sessions [completed] | note | notes | promote | comment [line/file/general] | threads | files | save | close | complete | submit | reconcile | sync | export [notes/review]"
    )
  end
end

-- Private ---------------------------------------------------------------------

function Private.refresh_sources()
  local contexts = require("lgtm.services.state")
  local buffers = vim.tbl_keys(contexts.notes)
  for _, buf in ipairs(buffers) do
    require("lgtm.app").restore_notes(buf)
  end
  if contexts.current then
    require("lgtm.app").refresh()
  end
end
return M
