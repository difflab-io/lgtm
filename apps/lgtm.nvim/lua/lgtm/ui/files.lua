-- State -----------------------------------------------------------------------

local M = {}

-- Public ----------------------------------------------------------------------

function M.open(options)
  -- State ---------------------------------------------------------------------

  if not require("lgtm.ui.theme").available() then
    return
  end
  local Split = require("nui.split")
  local Tree = require("nui.tree")
  local Line = require("nui.line")
  local origin = vim.api.nvim_get_current_win()
  local split, tree
  local view = {}
  local selected, map

  -- Render --------------------------------------------------------------------

  local function render_file(node)
    local line = Line()
    line:append(node.selected and "▶ " or "  ", "LgtmInfo")
    line:append(node.reviewed and "✓ " or "· ", node.reviewed and "LgtmSuccess" or "LgtmMuted")
    line:append(node.changed and "M " or "= ", "LgtmMuted")
    line:append(node.path, "LgtmNormal")
    return line
  end

  local function render()
    return Tree({
      bufnr = split.bufnr,
      get_node_id = function(node)
        return node.path
      end,
      prepare_node = render_file,
    })
  end

  -- Lifecycle -----------------------------------------------------------------

  local function mount()
    split = Split({
      relative = {
        type = "win",
        winid = options.anchor_win,
      },
      position = "left",
      size = 30,
      enter = options.focus,
      buf_options = {
        filetype = "lgtmfiles",
        swapfile = false,
      },
      win_options = {
        winfixwidth = true,
        number = false,
        relativenumber = false,
        wrap = false,
        winbar = "LGTM files · Enter select · m reviewed",
      },
    })
    split:mount()
    view.buf, view.win = split.bufnr, split.winid
    tree = render()
    map("<CR>", function()
      local path = selected()
      if path then
        options.on_select(path)
      end
    end)
    map("m", function()
      local path = selected()
      if path then
        options.on_toggle_reviewed(path)
      end
    end)
    for _, key in ipairs({ "f", "q" }) do
      map(key, options.on_close)
    end
    for key, direction in pairs({ ["<Tab>"] = 1, ["<S-Tab>"] = -1, ["]f"] = 1, ["[f"] = -1 }) do
      map(key, function()
        options.on_navigate(direction, key == "<Tab>" or key == "<S-Tab>")
      end)
    end
    if not options.focus and vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
    end
  end

  -- Methods -------------------------------------------------------------------

  function view.update(items, selected_path)
    tree:set_nodes(vim.tbl_map(function(item)
      return Tree.Node(item)
    end, items))
    tree:render()
    local _, row = tree:get_node(selected_path or "")
    if row and vim.api.nvim_win_is_valid(split.winid) then
      vim.api.nvim_win_set_cursor(split.winid, { row, 0 })
    end
  end

  function view.close()
    split:unmount()
    view.win = nil
  end

  -- Private -------------------------------------------------------------------

  selected = function()
    local node = tree:get_node(vim.api.nvim_win_get_cursor(split.winid)[1])
    return node and node.path
  end

  map = function(key, callback)
    split:map("n", key, vim.schedule_wrap(callback), {
      noremap = true,
      silent = true,
    })
  end

  mount()
  return view
end

return M
