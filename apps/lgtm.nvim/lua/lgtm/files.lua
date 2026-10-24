-- Backend-provided files; this pane works equally for Git and arbitrary directories.
-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local api = vim.api

-- Local marks survive editor restarts; GitHub owns PR viewer state.

-- Selected bodies supply the original identity. Hidden files need only bounded metadata checks.

-- Public ----------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.render(state)
  if state.native_explorer then
    local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
    if not panel or not panel.tree or not api.nvim_buf_is_valid(panel.bufnr) then
      return
    end
    if state.reviewed_panel_buf ~= panel.bufnr then
      state.reviewed_panel_buf = panel.bufnr
      vim.keymap.set("n", "m", function()
        local node = panel.tree:get_node()
        if node and node.data and node.data.type ~= "group" and node.data.type ~= "directory" then
          require("lgtm.review").reviewed(node.data.path)
        end
      end, { buffer = panel.bufnr, desc = "Toggle file reviewed (seen)", silent = true })
      api.nvim_buf_attach(panel.bufnr, false, {
        on_lines = function()
          vim.schedule(function()
            if require("lgtm.services.state").current == state then
              M.render(state)
            end
          end)
        end,
      })
    end
    local ns = api.nvim_create_namespace("lgtm.reviewed")
    api.nvim_buf_clear_namespace(panel.bufnr, ns, 0, -1)
    for _, file in ipairs(state.files or {}) do
      if
        state.reviewed
        and state.reviewed[file.path]
        and state.reviewed[file.path] ~= "github"
        and file.contentHash
        and not Private.same_content(state.reviewed[file.path], file.contentHash)
      then
        require("lgtm.services.files").invalidate_reviewed(state, file.path)
      end
    end
    for row = 1, api.nvim_buf_line_count(panel.bufnr) do
      local node = panel.tree:get_node(row)
      if node and node.data and state.reviewed and state.reviewed[node.data.path] then
        api.nvim_buf_set_extmark(panel.bufnr, ns, row - 1, 0, {
          virt_text = { { "✓", "LgtmSuccess" } },
          virt_text_pos = "right_align",
        })
      end
    end
    return
  end
  local pane = state.explorer
  if not pane or not api.nvim_buf_is_valid(pane.buf) then
    return
  end
  local items = {}
  state.reviewed = state.reviewed or {}
  for _, file in ipairs(state.files or {}) do
    if
      state.reviewed[file.path]
      and state.reviewed[file.path] ~= "github"
      and file.contentHash
      and not Private.same_content(state.reviewed[file.path], file.contentHash)
    then
      require("lgtm.services.files").invalidate_reviewed(state, file.path)
    end
    items[#items + 1] = {
      path = file.path,
      selected = state.file and state.file.path == file.path,
      reviewed = state.reviewed[file.path] ~= nil,
      changed = state.kind == "pr" or (file.baseContent or "") ~= (file.content or ""),
    }
  end
  pane.update(items, state.file and state.file.path)
end

---@param state lgtm.ReviewState
function M.toggle_reviewed(state, file)
  local pane = state.explorer
  local cursor = pane
    and pane.win == api.nvim_get_current_win()
    and api.nvim_win_get_cursor(pane.win)
  state.reviewed = state.reviewed or {}
  local previous = state.reviewed[file.path]
  if previous then
    state.reviewed[file.path] = nil
  else
    state.reviewed[file.path] = file.contentHash
  end
  local selected = state.reviewed[file.path]
  require("lgtm.services.files").save_reviewed(state, file.path, selected, function(err)
    if err and state.reviewed[file.path] == selected then
      state.reviewed[file.path] = previous
      M.render(state)
    end
  end)
  M.render(state)
  if cursor then
    api.nvim_win_set_cursor(pane.win, cursor)
  end
end

-- CodeDiff updates working/index buffers independently of file selection.
---@param state lgtm.ReviewState
function M.observe(state, buf)
  state.reviewed_buffers = state.reviewed_buffers or {}
  if state.reviewed_buffers[buf] then
    return
  end
  state.reviewed_buffers[buf] = true
  api.nvim_buf_attach(buf, false, {
    on_lines = function()
      vim.schedule(function()
        if
          require("lgtm.services.state").current ~= state
          or not api.nvim_buf_is_valid(buf)
          or not state.file
          or not state.panes
          or state.panes.new.buf ~= buf
        then
          return
        end
        -- Invalidate immediately. A bounded selected-source read runs after CodeDiff settles.
        state.reviewed = state.reviewed or {}
        require("lgtm.services.files").invalidate_reviewed(state, state.file.path)
        state.views = {}
        state.source_generation = (state.source_generation or 0) + 1
        if state.render then
          state.render()
        end
        state.source_refresh = (state.source_refresh or 0) + 1
        local generation = state.source_refresh
        vim.defer_fn(function()
          if
            require("lgtm.services.state").current == state
            and state.source_refresh == generation
            and state.bind_panes
          then
            state.bind_panes()
          end
        end, 80)
      end)
    end,
    on_detach = function()
      state.reviewed_buffers[buf] = nil
    end,
  })
end

---@param state lgtm.ReviewState
function M.select(state, path)
  state.focus_side = "new"
  state.show_file(path)
end

---@param state lgtm.ReviewState
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

---@param state lgtm.ReviewState
function M.show(state, focus)
  if not state or not state.tab or not api.nvim_tabpage_is_valid(state.tab) then
    return
  end
  state.explorer_visible = true
  local origin = api.nvim_get_current_win()
  local pane = state.explorer
  if not pane or not pane.win or not api.nvim_win_is_valid(pane.win) then
    api.nvim_set_current_tabpage(state.tab)
    pane = require("lgtm.ui.files").open({
      anchor_win = state.panes.old.win,
      focus = focus,
      on_select = function(path)
        M.select(state, path)
      end,
      on_toggle_reviewed = function(path)
        require("lgtm.review").reviewed(path)
      end,
      on_navigate = function(direction, unreviewed)
        M.navigate(state, direction, unreviewed)
      end,
      on_close = function()
        M.toggle(state)
      end,
    })
    state.explorer = pane
  end
  if not pane then
    return
  end
  M.render(state)
  if focus then
    api.nvim_set_current_win(pane.win)
  elseif api.nvim_win_is_valid(origin) then
    api.nvim_set_current_win(origin)
  end
end

---@param state lgtm.ReviewState
function M.toggle(state)
  if not state then
    return
  end
  if state.native_explorer then
    local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
    if panel then
      require("codediff.ui.explorer").toggle_visibility(panel)
    end
    return
  end
  local pane = state.explorer
  if pane and pane.win and api.nvim_win_is_valid(pane.win) then
    pane.close()
    state.explorer_visible = false
    if state.panes and api.nvim_win_is_valid(state.panes.new.win) then
      api.nvim_set_current_win(state.panes.new.win)
    end
  else
    M.show(state, true)
  end
end

---@param state lgtm.ReviewState
function M.attach(state, buf)
  local function map(key, callback)
    require("lgtm.bindings").set(
      state,
      "n",
      key,
      vim.schedule_wrap(callback),
      { buffer = buf, silent = true }
    )
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

-- Private ---------------------------------------------------------------------

-- JSON key order can differ across editor processes. Compare source values.
function Private.same_content(first, second)
  if first == second then
    return true
  end
  local first_ok, first_value = pcall(vim.json.decode, first)
  local second_ok, second_value = pcall(vim.json.decode, second)
  return first_ok
    and second_ok
    and type(first_value) == "table"
    and vim.deep_equal(first_value, second_value)
end

return M
