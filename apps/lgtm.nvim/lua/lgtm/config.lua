local Config = {}

-- State -----------------------------------------------------------------------

local defaults = {
  seed_prs = nil,
  keymaps = {
    prefix = "<leader>r",
    comment = "<leader>rc",
    pr_select = "<leader>rp",
    threads_toggle = "<leader>rt",
    thread_open = "<leader>ro",
    thread_resolve = "<leader>rx",
    pr_submit = "<leader>rs",
    pr_approve = "<leader>ra",
    pr_request_changes = "<leader>rr",
    next_thread = "]c",
    prev_thread = "[c",
  },
}

local current = vim.deepcopy(defaults)

-- Methods ---------------------------------------------------------------------

function Config.setup(opts)
  current = vim.tbl_deep_extend("force", vim.deepcopy(defaults), opts or {})
  return current
end

function Config.get()
  return current
end

function Config.defaults()
  return vim.deepcopy(defaults)
end

return Config
