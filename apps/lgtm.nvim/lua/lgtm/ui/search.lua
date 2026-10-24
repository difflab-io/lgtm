local M = {}

-- Items contain display labels and opaque IDs. The caller owns candidate lookup.
function M.open(options)
  local theme = require("lgtm.ui.theme")
  if not theme.available() then
    return
  end
  if M.last and not M.last.closed then
    M.last.close()
  end
  local n = require("nui-components")
  local origin, cursor = vim.api.nvim_get_current_win(), vim.api.nvim_win_get_cursor(0)
  local view =
    { entries = options.items, filtered = options.items, index = 1, query = "", closed = false }
  local function nodes(items)
    if #items == 0 then
      return { n.option("(no matches)", { id = "empty" }) }
    end
    return vim.tbl_map(function(item)
      return n.option(item.label, { id = item.id })
    end, items)
  end
  local signal = n.create_signal({ items = nodes(options.items) })
  local function size()
    return {
      width = math.max(3, math.min(78, vim.o.columns - 2)),
      height = math.max(6, math.min(15, vim.o.lines - 3)),
    }
  end
  local renderer = n.create_renderer(size())
  local input, list, resize
  local function close()
    if view.closed then
      return
    end
    view.closed = true
    if resize then
      vim.api.nvim_del_autocmd(resize)
      resize = nil
    end
    renderer:close()
    vim.cmd.stopinsert()
    if vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
      pcall(vim.api.nvim_win_set_cursor, origin, cursor)
    end
  end
  view.close = close
  local function choose()
    if view.closed or #view.filtered == 0 then
      return
    end
    local node = list:get_focused_node()
    local id = node and node.id
    if id then
      close()
      vim.schedule(function()
        options.on_select(id)
      end)
    end
  end
  local function navigate(direction)
    if #view.filtered == 0 then
      return
    end
    list:get_actions()[direction > 0 and "on_focus_next" or "on_focus_prev"]()
  end
  local Input = n.text_input:extend("LgtmSearchInput")
  function Input:mappings()
    return {
      { mode = { "n", "i" }, key = "<CR>", handler = choose },
      {
        mode = { "n", "i" },
        key = { "<Down>", "<C-n>" },
        handler = function()
          navigate(1)
        end,
      },
      {
        mode = { "n", "i" },
        key = { "<Up>", "<C-p>" },
        handler = function()
          navigate(-1)
        end,
      },
    }
  end
  input = Input(
    vim.tbl_extend("force", theme.get_component_options(), {
      id = "query",
      size = 1,
      max_lines = 1,
      autofocus = true,
      on_mount = function(component)
        view.buf, view.win = component.bufnr, component.winid
      end,
      on_change = vim.schedule_wrap(function(query)
        if view.closed then
          return
        end
        view.query, view.index, view.filtered = query, 1, {}
        for _, item in ipairs(options.items) do
          if item.label:lower():find(query:lower(), 1, true) then
            view.filtered[#view.filtered + 1] = item
          end
        end
        signal.items = nodes(view.filtered)
        local tree = list:get_tree()
        if tree and list.winid and vim.api.nvim_win_is_valid(list.winid) then
          list:set_focused_node(tree:get_node(1))
          vim.api.nvim_win_set_cursor(list.winid, { 1, 0 })
        end
      end),
    }),
    theme.get_popup_options("Search " .. options.title)
  )
  local Select = n.select:extend("LgtmSearchSelect")
  function Select:hl_group(name)
    if name == "NodeFocused" or name == "OptionSelected" then
      return "LgtmSelection"
    end
    if name == "Separator" then
      return "LgtmMuted"
    end
    return "LgtmNormal"
  end
  list = Select(
    vim.tbl_extend("force", theme.get_component_options(), {
      id = "results",
      flex = 1,
      data = signal.items,
      on_mount = function(component)
        view.list_buf, view.list_win = component.bufnr, component.winid
      end,
      on_change = function(node)
        for index, item in ipairs(view.filtered) do
          if node and node.id == item.id then
            view.index = index
            return
          end
        end
      end,
      on_select = choose,
    }),
    theme.get_popup_options(nil, " Enter select · Esc cancel · Up/Down navigate ")
  )
  renderer:add_mappings({
    { mode = { "n", "i", "x" }, key = "<Esc>", handler = close },
    { mode = "n", key = "q", handler = close },
  })
  renderer:on_unmount(function()
    view.closed = true
    if resize then
      vim.api.nvim_del_autocmd(resize)
      resize = nil
    end
  end)
  resize = vim.api.nvim_create_autocmd("VimResized", {
    callback = function()
      if not view.closed then
        renderer:set_size(size())
      end
    end,
  })
  renderer:render(n.rows({ flex = 1 }, input, list))
  M.last = view
  return view
end
return M
