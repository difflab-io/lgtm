local M = {}
function M.setup(opts)
  require("lgtm.config").setup(opts)
  if vim.fn.has("nvim-0.11") == 0 then
    error("lgtm.nvim requires Neovim >= 0.11")
  end
  local ok, notify = pcall(require, "notify")
  if ok then
    vim.notify = notify
  end
  if pcall(require, "which-key") then
    require("lgtm.keymaps").setup()
  end
  local group = vim.api.nvim_create_augroup("Lgtm", { clear = true })
  vim.api.nvim_create_autocmd("BufEnter", {
    group = group,
    callback = function(event)
      require("lgtm.notes").restore(event.buf)
    end,
  })
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufWritePost" }, {
    group = group,
    callback = function(event)
      local state = require("lgtm.notes").states[event.buf]
      if not state then
        return
      end
      if event.event == "BufWritePost" then
        state.refresh()
      else
        vim.api.nvim_buf_clear_namespace(event.buf, require("lgtm.threads").namespace, 0, -1)
      end
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
    review = review.branch,
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
    export = function()
      require("lgtm.export").run(rest ~= "" and rest or "review")
    end,
  }
  if commands[action] then
    commands[action]()
  else
    require("lgtm.notifications").show(
      "Lgtm: review | directory [path] | prs [URL] | sessions [completed] | note | notes | promote | comment [line/file/general] | threads | files | save | close | complete | submit | reconcile | export [notes/review]"
    )
  end
end
return M
