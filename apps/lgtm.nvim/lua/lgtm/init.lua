local M = {}
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
      local contexts = require("lgtm.context")
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
              if state.reload_pending then
                require("lgtm.locations").reload(state, function(err)
                  if err then
                    require("lgtm.notifications").error(err)
                  end
                end)
              else
                state.wait_ready()
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
      local state = require("lgtm.context").tabs[vim.api.nvim_get_current_tabpage()]
      if state then
        require("lgtm.codediff").suspend(state)
      end
    end,
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = "CodeDiffClose",
    callback = function(event)
      local state = require("lgtm.context").tabs[event.data.tabpage]
      if state then
        require("lgtm.codediff").release(state)
      end
    end,
  })
  vim.api.nvim_create_autocmd("TabClosed", {
    group = group,
    callback = function()
      for tab, state in pairs(require("lgtm.context").tabs) do
        if not vim.api.nvim_tabpage_is_valid(tab) then
          require("lgtm.codediff").release(state)
          local contexts = require("lgtm.context")
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
      require("lgtm.notes").restore(event.buf)
    end,
  })
  vim.api.nvim_create_autocmd("BufFilePre", {
    group = group,
    callback = function(event)
      vim.b[event.buf].lgtm_previous_path = vim.api.nvim_buf_get_name(event.buf)
    end,
  })
  vim.api.nvim_create_autocmd("BufFilePost", {
    group = group,
    callback = function(event)
      local previous = vim.b[event.buf].lgtm_previous_path
      vim.b[event.buf].lgtm_previous_path = nil
      if previous and previous ~= "" then
        require("lgtm.locations").rename(previous, vim.api.nvim_buf_get_name(event.buf))
      end
    end,
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = { "MiniFilesActionRename", "LgtmPathRenamed" },
    callback = function(event)
      if event.data and event.data.from and event.data.to then
        require("lgtm.locations").rename(event.data.from, event.data.to)
      end
    end,
  })
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufWritePost" }, {
    group = group,
    callback = function(event)
      local notes = require("lgtm.notes")
      local state = notes.states[event.buf]
      if not state or notes.owns_review_buffer(event.buf) then
        return
      end
      if event.event == "BufWritePost" then
        state.refresh()
      else
        state.views = {}
        state.render_generation = (state.render_generation or 0) + 1
        vim.api.nvim_buf_clear_namespace(event.buf, require("lgtm.threads").namespace, 0, -1)
      end
    end,
  })
  vim.api.nvim_create_autocmd({ "WinResized", "ColorScheme" }, {
    group = group,
    callback = function()
      require("lgtm.ui.theme").highlights()
      vim.schedule(function()
        local state = require("lgtm.context").current
        if state and state.render_ready and state.render then
          state.render()
        end
        for buf, notes in pairs(require("lgtm.notes").states) do
          if
            vim.api.nvim_buf_is_valid(buf)
            and #vim.fn.win_findbuf(buf) > 0
            and notes.conversation
            and not require("lgtm.notes").owns_review_buffer(buf)
          then
            require("lgtm.threads").render(notes, buf, notes.path, "new")
          end
        end
      end)
    end,
  })
  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = group,
    callback = function()
      require("lgtm.transport.connect").cancel_all()
      local state = require("lgtm.context").current
      if state and state.temp_dir then
        vim.fn.delete(state.temp_dir, "rf")
      end
    end,
  })
end

function M.command(args, range)
  local action, rest = args:match("^(%S*)%s*(.-)$")
  local review = require("lgtm.review")
  local commands = {
    review = review.select,
    directory = function()
      review.directory(rest ~= "" and rest or nil)
    end,
    prs = function()
      require("lgtm.sessions").prs(rest ~= "" and rest or nil)
    end,
    sessions = function()
      require("lgtm.sessions").select(rest == "completed")
    end,
    note = function()
      require("lgtm.notes").add(range)
    end,
    notes = function()
      require("lgtm.notes").list()
    end,
    promote = function()
      require("lgtm.notes").promote()
    end,
    comment = function()
      review.comment(rest ~= "" and rest or "line", range)
    end,
    threads = review.threads,
    files = review.files,
    save = review.save,
    refresh = review.refresh,
    close = review.close,
    complete = review.complete,
    layout = function()
      require("lgtm.codediff").toggle(require("lgtm.context").current)
    end,
    submit = review.submit,
    reconcile = review.reconcile,
    sync = review.sync,
    export = function()
      require("lgtm.export").run(rest ~= "" and rest or "review")
    end,
  }
  if commands[action] then
    commands[action]()
  else
    require("lgtm.notifications").show(
      "Lgtm: review | directory [path] | prs [URL] | sessions [completed] | note | notes | promote | comment [line/file/general] | threads | files | save | close | complete | submit | reconcile | sync | export [notes/review]"
    )
  end
end
return M
