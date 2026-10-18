if vim.g.loaded_lgtm then
  return
end
vim.g.loaded_lgtm = true
vim.api.nvim_create_user_command("Lgtm", function(opts)
  require("lgtm").command(opts.args, opts.range > 0 and { opts.line1, opts.line2 } or nil)
end, {
  nargs = "*",
  range = true,
  complete = function(arg)
    return vim.tbl_filter(function(command)
      return command:sub(1, #arg) == arg
    end, {
      "review",
      "layout",
      "refresh",
      "directory",
      "prs",
      "sessions",
      "note",
      "notes",
      "promote",
      "comment",
      "threads",
      "files",
      "save",
      "close",
      "complete",
      "submit",
      "reconcile",
      "export",
    })
  end,
})
