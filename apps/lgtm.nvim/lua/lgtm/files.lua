-- Backend-provided files; this pane works equally for Git and arbitrary directories.
local M = {}
local api = vim.api

function M.render(state)
  local pane = state.explorer
  if not pane or not api.nvim_buf_is_valid(pane.buf) then
    return
  end
  local lines = {}
  state.reviewed = state.reviewed or {}
  for _, file in ipairs(state.files or {}) do
    if state.reviewed[file.path] and state.reviewed[file.path] ~= file.contentHash then
      state.reviewed[file.path] = nil
    end
    local changed = (file.baseContent or "") ~= (file.content or "")
    lines[#lines + 1] = string.format(
      "%s %s %s %s",
      state.file and state.file.path == file.path and "▶" or " ",
      state.reviewed[file.path] and "✓" or "·",
      changed and "M" or "=",
      file.path
    )
  end
  vim.bo[pane.buf].modifiable = true
  api.nvim_buf_set_lines(pane.buf, 0, -1, false, lines)
  vim.bo[pane.buf].modifiable = false
  if pane.win and api.nvim_win_is_valid(pane.win) and state.file then
    for i, file in ipairs(state.files) do
      if file.path == state.file.path then
        pcall(api.nvim_win_set_cursor, pane.win, { i, 0 })
        break
      end
    end
  end
end

function M.select(state, index)
  local file = (state.files or {})[index]
  if not file then
    return
  end
  state.focus_side = "new"
  state.show_file(file.path)
end

function M.navigate(state, direction, unreviewed)
  if not state or #(state.files or {}) == 0 then
    return
  end
  M.render(state)
  local current = 1
  for index, file in ipairs(state.files) do
    if state.file and file.path == state.file.path then
      current = index
      break
    end
  end
  local side = vim.b.lgtm and vim.b.lgtm.side or "new"
  for offset = 1, #state.files do
    local index = ((current - 1 + direction * offset) % #state.files) + 1
    local file = state.files[index]
    if not unreviewed or not state.reviewed[file.path] then
      state.focus_side = side
      state.show_file(file.path)
      return
    end
  end
  require("lgtm.notifications").show("All review files are marked reviewed")
end

function M.show(state, focus)
  if not state or not state.tab or not api.nvim_tabpage_is_valid(state.tab) then
    return
  end
  state.explorer_visible = true
  local origin = api.nvim_get_current_win()
  state.explorer = state.explorer or {}
  local pane = state.explorer
  if not pane.buf or not api.nvim_buf_is_valid(pane.buf) then
    pane.buf = api.nvim_create_buf(false, true)
    vim.bo[pane.buf].filetype = "lgtmfiles"
    vim.bo[pane.buf].bufhidden = "hide"
    vim.bo[pane.buf].swapfile = false
    local function map(key, callback)
      vim.keymap.set("n", key, vim.schedule_wrap(callback), { buffer = pane.buf, silent = true })
    end
    map("<CR>", function()
      M.select(state, api.nvim_win_get_cursor(pane.win)[1])
    end)
    map("f", function()
      M.toggle(state)
    end)
    map("q", function()
      M.toggle(state)
    end)
    map("<Tab>", function()
      M.navigate(state, 1, true)
    end)
    map("<S-Tab>", function()
      M.navigate(state, -1, true)
    end)
    map("]f", function()
      M.navigate(state, 1, false)
    end)
    map("[f", function()
      M.navigate(state, -1, false)
    end)
    map("m", function()
      local file = state.files[api.nvim_win_get_cursor(pane.win)[1]]
      if not file then
        return
      end
      state.reviewed = state.reviewed or {}
      state.reviewed[file.path] = state.reviewed[file.path] and nil or file.contentHash
      M.render(state)
    end)
  end
  if not pane.win or not api.nvim_win_is_valid(pane.win) then
    api.nvim_set_current_tabpage(state.tab)
    api.nvim_set_current_win(state.panes.old.win)
    vim.cmd("topleft 30vsplit")
    pane.win = api.nvim_get_current_win()
    api.nvim_win_set_buf(pane.win, pane.buf)
    vim.wo[pane.win].winfixwidth = true
    vim.wo[pane.win].number = false
    vim.wo[pane.win].relativenumber = false
    vim.wo[pane.win].wrap = false
    vim.wo[pane.win].winbar = "LGTM files · Enter select · m reviewed"
  end
  M.render(state)
  if focus then
    api.nvim_set_current_win(pane.win)
  elseif api.nvim_win_is_valid(origin) then
    api.nvim_set_current_win(origin)
  end
end

function M.toggle(state)
  if not state then
    return
  end
  local pane = state.explorer
  if pane and pane.win and api.nvim_win_is_valid(pane.win) then
    api.nvim_win_close(pane.win, true)
    pane.win = nil
    state.explorer_visible = false
    if state.panes and api.nvim_win_is_valid(state.panes.new.win) then
      api.nvim_set_current_win(state.panes.new.win)
    end
  else
    M.show(state, true)
  end
end

function M.attach(state, buf)
  local function map(key, callback)
    vim.keymap.set("n", key, vim.schedule_wrap(callback), { buffer = buf, silent = true })
  end
  map("f", function()
    M.toggle(state)
  end)
  map("<Tab>", function()
    M.navigate(state, 1, true)
  end)
  map("<S-Tab>", function()
    M.navigate(state, -1, true)
  end)
  map("]f", function()
    M.navigate(state, 1, false)
  end)
  map("[f", function()
    M.navigate(state, -1, false)
  end)
end

return M
