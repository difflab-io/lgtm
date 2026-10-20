-- Backend-provided files; this pane works equally for Git and arbitrary directories.
local M = {}
local api = vim.api

-- Selected bodies supply the original identity. Hidden files need only bounded metadata checks.
function M.validate_reviewed(state)
  if not state.git_root or not state.reviewed then
    return
  end
  local selected = {}
  for _, file in ipairs(state.files or {}) do
    if state.reviewed[file.path] then
      selected[#selected + 1] = file
    end
  end
  if #selected == 0 then
    return
  end
  state.reviewed_generation = (state.reviewed_generation or 0) + 1
  local generation = state.reviewed_generation
  local function invalidate(file, changed)
    if state.closed or state.reviewed_generation ~= generation then
      return
    end
    if changed then
      state.reviewed[file.path] = nil
      M.render(state)
    end
  end
  if state.mode == "working" then
    for _, file in ipairs(selected) do
      local expected = file.saved_stamp
      vim.uv.fs_stat(vim.fs.joinpath(state.git_root, file.path), function(err, stat)
        vim.schedule(function()
          local absent = expected == false and err and err:match("ENOENT")
          invalidate(
            file,
            not absent
              and (
                err ~= nil
                or not vim.deep_equal(expected, require("lgtm.sources").saved_stamp(stat))
              )
          )
        end)
      end)
    end
  elseif state.mode == "staged" then
    -- Bound each command even when a large review has many viewed files.
    for offset = 1, #selected, 500 do
      local batch = vim.list_slice(selected, offset, math.min(offset + 499, #selected))
      local args = {
        "git",
        "--no-optional-locks",
        "--literal-pathspecs",
        "-C",
        state.git_root,
        "ls-files",
        "--stage",
        "-z",
        "--",
      }
      for _, file in ipairs(batch) do
        args[#args + 1] = file.path
      end
      vim.system(args, { text = false, timeout = 5000 }, function(result)
        vim.schedule(function()
          local entries = {}
          for _, record in
            ipairs(vim.split(result.stdout or "", "\0", { plain = true, trimempty = true }))
          do
            local mode, oid, stage, path = record:match("^(%d+) (%x+) (%d)\t(.*)$")
            if path then
              entries[path] = stage == "0" and (mode == "100644" or mode == "100755") and oid
                or "unavailable"
            end
          end
          for _, file in ipairs(batch) do
            invalidate(file, result.code ~= 0 or (entries[file.path] or false) ~= file.index_oid)
          end
        end)
      end)
    end
  end
end

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
            if require("lgtm.context").current == state then
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
        and state.reviewed[file.path] ~= file.contentHash
      then
        state.reviewed[file.path] = nil
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
  local lines = {}
  state.reviewed = state.reviewed or {}
  for _, file in ipairs(state.files or {}) do
    if state.reviewed[file.path] and state.reviewed[file.path] ~= file.contentHash then
      state.reviewed[file.path] = nil
    end
    local changed = state.kind == "pr" or (file.baseContent or "") ~= (file.content or "")
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

function M.toggle_reviewed(state, file)
  local pane = state.explorer
  local cursor = pane
    and pane.win == api.nvim_get_current_win()
    and api.nvim_win_get_cursor(pane.win)
  state.reviewed = state.reviewed or {}
  if state.reviewed[file.path] then
    state.reviewed[file.path] = nil
  else
    state.reviewed[file.path] = file.contentHash
  end
  M.render(state)
  if cursor then
    api.nvim_win_set_cursor(pane.win, cursor)
  end
  require("lgtm.notifications").show(
    (state.reviewed[file.path] and "File marked reviewed: " or "File marked unreviewed: ")
      .. file.path
  )
end

-- CodeDiff updates working/index buffers independently of file selection.
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
          require("lgtm.context").current ~= state
          or not api.nvim_buf_is_valid(buf)
          or not state.file
          or not state.panes
          or state.panes.new.buf ~= buf
        then
          return
        end
        -- Invalidate immediately. A bounded selected-source read runs after CodeDiff settles.
        state.reviewed = state.reviewed or {}
        state.reviewed[state.file.path] = nil
        state.views = {}
        state.source_generation = (state.source_generation or 0) + 1
        if state.render then
          state.render()
        end
        state.source_refresh = (state.source_refresh or 0) + 1
        local generation = state.source_refresh
        vim.defer_fn(function()
          if
            require("lgtm.context").current == state
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
      require("lgtm.review").reviewed(file.path)
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
  if state.native_explorer then
    local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
    if panel then
      require("codediff.ui.explorer").toggle_visibility(panel)
    end
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

return M
