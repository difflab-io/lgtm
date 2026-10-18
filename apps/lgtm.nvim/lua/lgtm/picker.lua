local M = {}
-- Built-in search works without a fuzzy-picker plugin or another runtime dependency.
function M.searchable(title, entries, format, callback)
  if M.last and not M.last.closed then
    M.last.close()
  end
  local api = vim.api
  local origin = api.nvim_get_current_win()
  local cursor = api.nvim_win_get_cursor(origin)
  local width, height =
    math.max(20, math.min(76, vim.o.columns - 6)), math.max(3, math.min(10, vim.o.lines - 10))
  local col, row =
    math.max(1, math.floor((vim.o.columns - width) / 2)),
    math.max(1, math.floor((vim.o.lines - height - 5) / 2))
  local state = { entries = entries, filtered = entries, index = 1, query = "", closed = false }
  state.buf = api.nvim_create_buf(false, true)
  state.list_buf = api.nvim_create_buf(false, true)
  for _, buf in ipairs({ state.buf, state.list_buf }) do
    vim.bo[buf].buftype = "nofile"
    vim.bo[buf].bufhidden = "wipe"
    vim.bo[buf].filetype = "lgtmpicker"
  end
  state.list_win = api.nvim_open_win(state.list_buf, false, {
    relative = "editor",
    row = row + 3,
    col = col,
    width = width,
    height = height,
    border = "rounded",
    footer = " Enter select · Esc cancel · Up/Down navigate ",
  })
  state.win = api.nvim_open_win(state.buf, true, {
    relative = "editor",
    row = row,
    col = col,
    width = width,
    height = 1,
    border = "rounded",
    title = " Search " .. title .. " ",
  })
  local function render()
    if state.closed then
      return
    end
    state.query = table.concat(api.nvim_buf_get_lines(state.buf, 0, -1, false), " ")
    state.filtered = {}
    for _, item in ipairs(entries) do
      if format(item):lower():find(state.query:lower(), 1, true) then
        state.filtered[#state.filtered + 1] = item
      end
    end
    state.index = math.max(1, math.min(state.index, #state.filtered))
    local lines = {}
    for _, item in ipairs(state.filtered) do
      lines[#lines + 1] = format(item)
    end
    if #lines == 0 then
      lines = { "(no matches)" }
    end
    vim.bo[state.list_buf].modifiable = true
    api.nvim_buf_set_lines(state.list_buf, 0, -1, false, lines)
    vim.bo[state.list_buf].modifiable = false
    api.nvim_win_set_cursor(state.list_win, { state.index, 0 })
  end
  function state.close()
    if state.closed then
      return
    end
    state.closed = true
    vim.cmd.stopinsert()
    for _, win in ipairs({ state.win, state.list_win }) do
      if api.nvim_win_is_valid(win) then
        api.nvim_win_close(win, true)
      end
    end
    if api.nvim_win_is_valid(origin) then
      api.nvim_set_current_win(origin)
      pcall(api.nvim_win_set_cursor, origin, cursor)
    end
    vim.schedule(function()
      if api.nvim_win_is_valid(origin) and api.nvim_get_current_win() == origin then
        vim.cmd.stopinsert()
      end
    end)
  end
  local function choose()
    local index = api.nvim_get_current_win() == state.list_win
        and api.nvim_win_get_cursor(state.list_win)[1]
      or state.index
    local item = state.filtered[index]
    if item then
      state.close()
      vim.schedule(function()
        callback(item)
      end)
    end
  end
  local function navigate(delta)
    if #state.filtered == 0 then
      return
    end
    state.index = ((state.index - 1 + delta) % #state.filtered) + 1
    api.nvim_win_set_cursor(state.list_win, { state.index, 0 })
  end
  for _, buf in ipairs({ state.buf, state.list_buf }) do
    vim.keymap.set({ "n", "i" }, "<CR>", vim.schedule_wrap(choose), { buffer = buf })
    vim.keymap.set({ "n", "i" }, "<Esc>", vim.schedule_wrap(state.close), { buffer = buf })
    for key, delta in pairs({ ["<Down>"] = 1, ["<Up>"] = -1, ["<C-n>"] = 1, ["<C-p>"] = -1 }) do
      vim.keymap.set({ "n", "i" }, key, function()
        navigate(delta)
      end, { buffer = buf })
    end
  end
  vim.keymap.set("n", "q", vim.schedule_wrap(state.close), { buffer = state.list_buf })
  api.nvim_buf_attach(state.buf, false, {
    on_lines = function()
      vim.schedule(render)
    end,
  })
  render()
  M.last = state
  vim.cmd.startinsert()
  return state
end

-- vim.ui.select/input are replaceable by the user's preferred fuzzy picker.
function M.select(title, entries, format, callback)
  if #entries == 0 then
    require("lgtm.notifications").show(title .. ": no results")
    return
  end
  if title == "Pull requests" or title == "Local / GitHub sessions" then
    return M.searchable(title, entries, format, callback)
  end
  vim.ui.select(entries, { prompt = title, format_item = format }, function(item)
    if item then
      callback(item)
    end
  end)
end

function M.input(title, default, callback)
  vim.ui.input({ prompt = title, default = default }, function(value)
    if value ~= nil then
      callback(value)
    end
  end)
end
return M
