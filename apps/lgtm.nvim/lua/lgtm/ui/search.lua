-- State -----------------------------------------------------------------------

local M = {}

-- Public ----------------------------------------------------------------------

function M.open(options)
  -- State ---------------------------------------------------------------------

  local theme = require("lgtm.ui.theme")
  if not theme.available() then
    return
  end
  if M.last and not M.last.closed then
    M.last.close()
  end
  local n = require("nui-components")
  local origin = vim.api.nvim_get_current_win()
  local cursor = vim.api.nvim_win_get_cursor(0)
  local view = {
    entries = options.items,
    filtered = options.items,
    index = 1,
    query = "",
    closed = false,
  }
  local input, results, signal, renderer, resize
  local close, choose, navigate, filter, select, nodes, get_size

  -- Render --------------------------------------------------------------------

  local function render_query()
    local Query = n.text_input:extend("LgtmSearchInput")
    function Query:mappings()
      return {
        {
          mode = { "n", "i" },
          key = "<CR>",
          handler = choose,
        },
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
    return Query(
      vim.tbl_extend("force", theme.get_component_options(), {
        id = "query",
        size = 1,
        max_lines = 1,
        autofocus = true,
        on_mount = function(component)
          view.buf, view.win = component.bufnr, component.winid
        end,
        on_change = vim.schedule_wrap(filter),
      }),
      theme.get_popup_options("Search " .. options.title)
    )
  end

  local function render_results()
    local Results = n.select:extend("LgtmSearchSelect")
    function Results:hl_group(name)
      if name == "NodeFocused" or name == "OptionSelected" then
        return "LgtmSelection"
      end
      return name == "Separator" and "LgtmMuted" or "LgtmNormal"
    end
    return Results(
      vim.tbl_extend("force", theme.get_component_options(), {
        id = "results",
        flex = 1,
        data = signal.items,
        on_mount = function(component)
          view.list_buf, view.list_win = component.bufnr, component.winid
        end,
        on_change = select,
        on_select = choose,
      }),
      theme.get_popup_options(nil, " Enter select · Esc cancel · Up/Down navigate ")
    )
  end

  local function render()
    input = render_query()
    results = render_results()
    return n.rows({ flex = 1 }, input, results)
  end

  -- Lifecycle -----------------------------------------------------------------

  local function unmount()
    view.closed = true
    if resize then
      vim.api.nvim_del_autocmd(resize)
      resize = nil
    end
  end

  local function mount()
    signal = n.create_signal({ items = nodes(options.items) })
    renderer = n.create_renderer(get_size())
    renderer:add_mappings({
      {
        mode = { "n", "i", "x" },
        key = "<Esc>",
        handler = close,
      },
      {
        mode = "n",
        key = "q",
        handler = close,
      },
    })
    renderer:on_unmount(unmount)
    resize = vim.api.nvim_create_autocmd("VimResized", {
      callback = function()
        if not view.closed then
          renderer:set_size(get_size())
        end
      end,
    })
    renderer:render(render())
  end

  -- Methods -------------------------------------------------------------------

  close = function()
    if view.closed then
      return
    end
    unmount()
    renderer:close()
    vim.cmd.stopinsert()
    if vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
      pcall(vim.api.nvim_win_set_cursor, origin, cursor)
    end
  end

  choose = function()
    if view.closed or #view.filtered == 0 then
      return
    end
    local node = results:get_focused_node()
    if node and node.id then
      close()
      vim.schedule(function()
        options.on_select(node.id)
      end)
    end
  end

  navigate = function(direction)
    if #view.filtered > 0 then
      results:get_actions()[direction > 0 and "on_focus_next" or "on_focus_prev"]()
    end
  end

  filter = function(query)
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
    local tree = results:get_tree()
    if tree and results.winid and vim.api.nvim_win_is_valid(results.winid) then
      results:set_focused_node(tree:get_node(1))
      vim.api.nvim_win_set_cursor(results.winid, { 1, 0 })
    end
  end

  select = function(node)
    for index, item in ipairs(view.filtered) do
      if node and node.id == item.id then
        view.index = index
        return
      end
    end
  end

  -- Private -------------------------------------------------------------------

  nodes = function(items)
    if #items == 0 then
      return { n.option("(no matches)", { id = "empty" }) }
    end
    return vim.tbl_map(function(item)
      return n.option(item.label, { id = item.id })
    end, items)
  end

  get_size = function()
    return {
      width = math.max(3, math.min(78, vim.o.columns - 2)),
      height = math.max(6, math.min(15, vim.o.lines - 3)),
    }
  end

  view.close = close
  M.last = view
  mount()
  return view
end

return M
