if vim.g.loaded_lgtm then
  return
end
vim.g.loaded_lgtm = true

vim.api.nvim_create_user_command("Lgtm", function(cmd_opts)
  require("lgtm.app").dispatch_command(cmd_opts.fargs)
end, {
  nargs = "*",
  desc = "LGTM review commands",
})
