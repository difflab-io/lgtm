-- State -----------------------------------------------------------------------

local M = {}
M.defaults = {
  endpoint_url = nil, -- Discovered from $LGTM_HOME/server.port; standalone server needs explicit URL.
  identity = nil, -- { name = "Your name", email = "you@example.com" }
  timeout = 30,
  prefix = "<leader>r",
  mappings = {},
  ui = {
    border = "rounded",
    highlights = {
      normal = "NormalFloat",
      border = "FloatBorder",
      title = "Title",
      muted = "Comment",
      selection = "Visual",
      info = "DiagnosticInfo",
      success = "DiagnosticOk",
      warning = "DiagnosticWarn",
      error = "DiagnosticError",
    },
  },
  max_response_bytes = 16 * 1024 * 1024,
}
M.options = vim.deepcopy(M.defaults)
-- Public ----------------------------------------------------------------------

function M.setup(opts)
  M.options = vim.tbl_deep_extend("force", vim.deepcopy(M.defaults), opts or {})
end
return M
