-- State -----------------------------------------------------------------------

local M = {}

local api = vim.api

-- Both hosts share display records and stable comment IDs, without review policy.
-- Public ----------------------------------------------------------------------

function M.open(options)
  -- State ---------------------------------------------------------------------

  local theme = require("lgtm.ui.theme")
  if not theme.available() then
    return
  end
  local n, Tree, Line = require("nui-components"), require("nui.tree"), require("nui.line")
  local origin, cursor = api.nvim_get_current_win(), api.nvim_win_get_cursor(0)
  local view = {
    closed = false,
    rows = {},
    entries = {},
  }
  local tree, component, renderer, split, resize, signal
  local records, selected_id, comment_ids = options.threads, nil, {}
  local width = math.max(1, math.min(options.mode == "split" and 42 or 86, vim.o.columns - 6))
  local nodes, prepare, observe, selection, restore, move, bind

  -- Render --------------------------------------------------------------------

  local function render_header(thread, index)
    local heading = thread.status
      .. " · "
      .. thread.replies
      .. (thread.replies == 1 and " reply" or " replies")
    if #records > 1 then
      heading = "THREAD " .. index .. " / " .. #records .. " · " .. heading
    end
    if options.mode == "split" then
      heading = heading .. " · " .. thread.location
    end
    if thread.location_status then
      heading = thread.location_status:upper() .. " · " .. heading
    end
    local lines = {
      { " " .. heading, thread.resolved and "LgtmSuccess" or "LgtmInfo" },
      { string.rep("─", width), "LgtmBorder" },
    }
    for _, detail in ipairs(thread.details) do
      lines[#lines + 1] = { " " .. detail, "LgtmWarning" }
    end
    return Tree.Node({
      id = "thread:" .. thread.id,
      thread_id = thread.id,
      lines = lines,
    })
  end

  local function render_comment(thread, comment)
    local prefix = comment.root and " " or "   └─ "
    local lines = {
      {
        prefix .. comment.role:upper() .. " · " .. comment.author .. " · " .. comment.status,
        comment.root and "LgtmTitle" or "LgtmMuted",
      },
    }
    for _, text in ipairs(vim.split(comment.body, "\n", { plain = true })) do
      lines[#lines + 1] = { (comment.root and "   " or "      ") .. text, "LgtmNormal" }
    end
    lines[#lines + 1] = { "", "LgtmNormal" }
    return Tree.Node({
      id = "comment:" .. comment.id,
      thread_id = thread.id,
      comment_id = comment.id,
      role = comment.role,
      can_resolve = thread.can_resolve,
      lines = lines,
    })
  end

  local function render_selection_footer(entry)
    local label = "Select a comment with Tab"
    if entry and entry.role then
      label = entry.role:gsub("^reply", "Reply")
      if entry.role == "top-level comment" then
        label = "Top-level comment"
      end
    end
    local actions = { "Tab next", "c reply", "e edit", "dd delete" }
    if entry and entry.can_resolve then
      actions[#actions + 1] = "x resolve"
    end
    actions[#actions + 1] = "q close"
    return " " .. label .. " · " .. table.concat(actions, " · ") .. " "
  end

  nodes = function()
    local result = {}
    comment_ids = {}
    for index, thread in ipairs(records) do
      result[#result + 1] = render_header(thread, index)
      for _, comment in ipairs(thread.comments) do
        comment_ids[#comment_ids + 1] = comment.id
        result[#result + 1] = render_comment(thread, comment)
      end
    end
    if #result == 0 then
      result = { Tree.Node({ id = "empty", lines = { { "No comments", "LgtmMuted" } } }) }
    end
    return result
  end

  prepare = function(node)
    return vim.tbl_map(function(part)
      local line = Line()
      line:append(part[1], node.comment_id == selected_id and "LgtmSelection" or part[2])
      return line
    end, node.lines)
  end

  -- Lifecycle -----------------------------------------------------------------

  local function mount()
    if options.mode == "split" then
      split = require("nui.split")({
        relative = "editor",
        position = "right",
        size = width,
        enter = true,
        buf_options = { filetype = "lgtm" },
        win_options = {
          number = false,
          relativenumber = false,
        },
      })
      split:mount()
      theme.apply(split.winid)
      view.buf, view.win = split.bufnr, split.winid
      tree = Tree({
        bufnr = view.buf,
        nodes = nodes(),
        get_node_id = function(node)
          return node.id
        end,
        prepare_node = prepare,
      })
      tree:render()
      bind()
    else
      local ConversationTree = n.tree:extend("LgtmConversationTree")
      function ConversationTree:mappings()
        return {}
      end
      local function size()
        width = math.max(1, math.min(86, vim.o.columns - 6))
        local rows = 0
        for _, node in ipairs(nodes()) do
          for _, line in ipairs(node.lines) do
            rows = rows + math.max(1, math.ceil(vim.fn.strdisplaywidth(line[1]) / width))
          end
        end
        return { width = width + 2, height = math.max(3, math.min(rows + 2, vim.o.lines - 5)) }
      end
      renderer = n.create_renderer(size())
      signal = n.create_signal({ data = nodes() })
      component = ConversationTree(
        vim.tbl_extend("force", theme.get_component_options(), {
          id = "conversation",
          flex = 1,
          autofocus = true,
          data = signal.data,
          prepare_node = prepare,
          on_unmount = view.close,
          on_mount = function(c)
            view.buf, view.win = c.bufnr, c.winid
            tree = c:get_tree()
            bind()
          end,
        }),
        theme.get_popup_options(#records == 1 and "Review thread" or "Review threads")
      )
      renderer:on_unmount(function()
        if not view.closed then
          view.closed = true
          if resize then
            api.nvim_del_autocmd(resize)
            resize = nil
          end
          restore()
        end
      end)
      resize = api.nvim_create_autocmd("VimResized", {
        callback = function()
          if not view.closed then
            renderer:set_size(size())
            view.update(records)
          end
        end,
      })
      renderer:render(component)
    end
  end

  -- Methods -------------------------------------------------------------------

  function view.selected()
    if not view.win or not api.nvim_win_is_valid(view.win) then
      return nil
    end
    local node = tree:get_node(api.nvim_win_get_cursor(view.win)[1])
    return node
      and {
        thread_id = node.thread_id,
        comment_id = node.comment_id,
        role = node.role,
        can_resolve = node.can_resolve,
      }
  end

  function view.select_comment(id)
    local node, row = tree:get_node("comment:" .. id)
    if node and row and api.nvim_win_is_valid(view.win) then
      api.nvim_win_set_cursor(view.win, { row, 0 })
      selection()
    end
  end

  function view.update(next_records)
    if view.closed then
      return
    end
    records = next_records
    if signal then
      signal.data = nodes()
    else
      tree:set_nodes(nodes())
      tree:render()
    end
    view.select_comment(selected_id or comment_ids[1] or "")
    observe()
  end

  function view.close()
    if view.closed then
      return
    end
    view.closed = true
    if resize then
      api.nvim_del_autocmd(resize)
      resize = nil
    end
    if renderer then
      renderer:close()
    else
      split:unmount()
    end
    restore()
  end

  -- Private -------------------------------------------------------------------

  observe = function()
    view.rows, view.entries = {}, {}
    for _, node in ipairs(tree:get_nodes()) do
      local _, first, last = tree:get_node(node.id)
      if first then
        local entry = {
          thread_id = node.thread_id,
          comment_id = node.comment_id,
          role = node.role,
          line = first,
          last_line = last,
        }
        for row = first, last do
          view.rows[row] = entry
        end
        if node.comment_id then
          view.entries[#view.entries + 1] = entry
        end
      end
    end
    if options.on_change then
      options.on_change(view)
    end
  end

  selection = function()
    local entry = view.selected()
    local next_id = entry and entry.comment_id
    if next_id ~= selected_id then
      selected_id = next_id
      tree:render()
    end
    if component then
      component:set_border_text("bottom", render_selection_footer(entry), "left")
    end
    observe()
  end

  restore = function()
    if api.nvim_win_is_valid(origin) then
      api.nvim_set_current_win(origin)
      pcall(api.nvim_win_set_cursor, origin, cursor)
    end
  end

  move = function(direction)
    local index = 0
    for i, id in ipairs(comment_ids) do
      if id == selected_id then
        index = i
        break
      end
    end
    if #comment_ids > 0 then
      view.select_comment(comment_ids[(index - 1 + direction) % #comment_ids + 1])
    end
  end

  bind = function()
    local function map(key, callback)
      local mapped = options.mappings[key] or key
      vim.keymap.set("n", mapped, vim.schedule_wrap(callback), {
        buffer = view.buf,
        silent = true,
      })
    end
    for key, action in pairs({ c = "reply", e = "edit", dd = "delete", x = "resolve" }) do
      map(key, function()
        local selected = view.selected()
        if selected then
          options.on_action(action, selected.thread_id, selected.comment_id)
        end
      end)
    end
    map("q", view.close)
    map("<Esc>", view.close)
    map("<Tab>", function()
      move(1)
    end)
    map("<S-Tab>", function()
      move(-1)
    end)
    if options.mode == "split" then
      map("<CR>", function()
        local selected = view.selected()
        if selected then
          options.on_navigate(selected.thread_id)
        end
      end)
    end
    vim.keymap.set("n", "j", "gj", {
      buffer = view.buf,
      silent = true,
    })
    vim.keymap.set("n", "k", "gk", {
      buffer = view.buf,
      silent = true,
    })
    api.nvim_create_autocmd("CursorMoved", { buffer = view.buf, callback = selection })
    vim.bo[view.buf].syntax = "markdown"
    vim.wo[view.win].wrap, vim.wo[view.win].linebreak, vim.wo[view.win].cursorline =
      true, true, false
    view.select_comment(comment_ids[1] or "")
    observe()
    if options.on_ready then
      options.on_ready()
    end
  end

  mount()
  return view
end

return M
