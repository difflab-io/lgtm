-- State -----------------------------------------------------------------------

local M = {}

local api = vim.api

-- Local marks survive editor restarts; GitHub owns PR viewer state.

-- Selected bodies supply the original identity. Hidden files need only bounded metadata checks.

-- Render ----------------------------------------------------------------------

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
          require("lgtm.app").reviewed(node.data.path)
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
  if state.file_selection then
    state.file_selection.path = state.file and state.file.path or ""
  end
end

-- Lifecycle -------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.select(state, path)
  state.focus_side = "new"
  state.show_file(path)
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

-- Methods ---------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.toggle_reviewed(state, file)
  local pane = state.explorer
  local cursor = pane
    and pane.win == api.nvim_get_current_win()
    and api.nvim_win_get_cursor(pane.win)
  require("lgtm.services.files_service").toggle_reviewed(state, file)
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
        require("lgtm.services.files_service").invalidate_reviewed(state, state.file.path)
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
  require("lgtm.ui.notifications").show("All review files are marked reviewed")
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
    state.file_selection =
      require("nui-components.signal").create({ path = state.file and state.file.path or "" })
    pane = require("lgtm.ui.files").open({
      data = require("lgtm.services.files_service")
        .get_data(state)
        :combine_latest(state.file_selection.path, function(data, path)
          return vim.tbl_map(function(file)
            return {
              path = file.path,
              selected = file.path == path,
              reviewed = data.reviewed[file.path] ~= nil,
              changed = state.kind == "pr" or (file.baseContent or "") ~= (file.content or ""),
            }
          end, data.files)
        end),
      anchor_win = state.panes.old.win,
      focus = focus,
      on_select = function(path)
        M.select(state, path)
      end,
      on_toggle_reviewed = function(path)
        require("lgtm.app").reviewed(path)
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
return M
