local M = {}

-- Original source is displayed as a scratch buffer; project files remain untouched.
function M.open(result, anchor)
  vim.cmd("botright new")
  local snapshot = result.snapshot
  local name = "lgtm://snapshot/" .. snapshot.commitOid .. "/" .. snapshot.path
  local buf = vim.fn.bufnr(name)
  if buf == -1 then
    buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_name(buf, name)
  end
  vim.api.nvim_win_set_buf(0, buf)
  vim.bo[buf].modifiable = true
  local content = (result.content or ""):gsub("\r\n", "\n")
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(content, "\n", { plain = true }))
  vim.bo[buf].buftype = "nofile"
  vim.bo[buf].bufhidden = "wipe"
  vim.bo[buf].swapfile = false
  vim.bo[buf].filetype = vim.filetype.match({ filename = snapshot.path }) or ""
  vim.bo[buf].modifiable = false
  vim.bo[buf].readonly = true
  vim.b[buf].lgtm_snapshot = snapshot
  local line = math.min(tonumber(anchor and anchor.start) or 1, vim.api.nvim_buf_line_count(buf))
  vim.api.nvim_win_set_cursor(0, { line, 0 })
  vim.keymap.set(
    "n",
    "q",
    "<cmd>close<cr>",
    { buffer = buf, silent = true, desc = "Close original source" }
  )
end

return M
