-- Adapter to pinned CodeDiff views. Git reviews use native revision buffers and explorer.
-- State -----------------------------------------------------------------------

local M = {}
local api = vim.api

-- Translate an annotation boundary through CodeDiff's actual edit ranges. A comment
-- inside an insertion/deletion pads the other pane at that edit's adjacent real line.
-- Public ----------------------------------------------------------------------

function M.other_line(line, source, changes, target_count)
  if line == 0 then
    return 0
  end
  local target = source == "old" and "modified" or "original"
  source = source == "old" and "original" or "modified"
  local delta = 0
  for _, change in ipairs(changes or {}) do
    local from, to = change[source], change[target]
    if line < from.start_line then
      break
    end
    if line < from.end_line then
      local mapped = to.start_line - 1
      if to.end_line > to.start_line then
        mapped = to.start_line + math.min(line - from.start_line, to.end_line - to.start_line - 1)
      end
      return math.max(0, math.min(mapped, target_count))
    end
    delta = to.end_line - from.end_line
  end
  return math.max(0, math.min(line + delta, target_count))
end

-- Mirror CodeDiff's metadata after native refreshes. Reuse loaded identities until a selected-source read replaces them.
---@param state lgtm.ReviewState
function M.sync_files(state, status)
  local previous = {}
  for _, file in ipairs(state.files or {}) do
    if file.group then
      previous[file.group .. ":" .. file.path] = file
    end
  end
  local files = {}
  for _, group in ipairs({ "unstaged", "staged" }) do
    for _, entry in ipairs(status[group] or {}) do
      local file = previous[group .. ":" .. entry.path] or {}
      if file.previousPath ~= entry.old_path and state.reviewed then
        state.reviewed[entry.path] = nil
      end
      file.path, file.status, file.previousPath = entry.path, entry.status, entry.old_path
      file.confirmedRename = entry.old_path ~= nil and entry.status:match("^R") ~= nil
      file.group = group
      file.native_entry = vim.tbl_extend("force", entry, { group = group })
      files[#files + 1] = file
    end
  end
  state.files = files
end

---@param state lgtm.ReviewState
function M.open(state, file, native_config)
  state.render_generation = (state.render_generation or 0) + 1
  local generation = state.render_generation
  state.render_ready = false
  local ok, view = pcall(require, "codediff.ui.view")
  if not ok then
    require("lgtm.notifications").error(
      "Install esmuellert/codediff.nvim and its native library; see :checkhealth lgtm"
    )
    return
  end
  local paths = require("codediff.core.path")
  local config
  if native_config then
    config = native_config
  elseif state.git_root then
    config = {
      git_root = state.git_root,
      original = file.status == "added" and paths.empty()
        or paths.make_ref(
          (file.previousPath and file.previousPath ~= "" and file.previousPath or file.path),
          state.git_root
        ),
      modified = file.status == "removed" and paths.empty()
        or paths.make_ref(file.path, state.git_root),
      original_revision = state.base,
      modified_revision = state.head,
      layout = "side-by-side",
    }
  else
    state.temp_dir = state.temp_dir or vim.fn.tempname()
    vim.fn.mkdir(state.temp_dir, "p")
    local token = vim.fn.sha256(file.path .. (file.contentHash or file.content or "")):sub(1, 16)
    if not state.snapshot_group then
      state.snapshot_group = api.nvim_create_augroup(
        "LgtmSnapshots" .. vim.fn.sha256(state.temp_dir):sub(1, 12),
        { clear = true }
      )
      api.nvim_create_autocmd("BufReadPost", {
        group = state.snapshot_group,
        pattern = state.temp_dir .. "/*",
        callback = function(event)
          vim.bo[event.buf].buftype = "nofile"
          vim.bo[event.buf].bufhidden = "wipe"
          vim.bo[event.buf].swapfile = false
        end,
      })
    end
    local original, modified =
      state.temp_dir .. "/" .. token .. "-old", state.temp_dir .. "/" .. token .. "-new"
    vim.fn.writefile(vim.split(file.baseContent or "", "\n", { plain = true }), original, "b")
    vim.fn.writefile(vim.split(file.content or "", "\n", { plain = true }), modified, "b")
    config = {
      original = paths.make_ref(original),
      modified = paths.make_ref(modified),
      layout = "side-by-side",
    }
  end
  local function ready()
    if generation ~= state.render_generation or state.closed then
      return
    end
    local lifecycle = require("codediff.ui.lifecycle")
    local session = lifecycle.get_session(state.tab or api.nvim_get_current_tabpage())
    if not session then
      return
    end
    if state.tab and state.tab ~= api.nvim_get_current_tabpage() then
      return
    end
    if state.native_explorer and session.panel and session.panel.data.status_result then
      M.sync_files(state, session.panel.data.status_result)
      local selected = session.panel.data.current_selection
      if #state.files == 0 and not selected then
        local was_empty = state.empty
        M.suspend(state)
        state.file, state.empty = nil, true
        state.render_ready = true
        state.views, state.observe_sources = {}, nil
        state.render = function()
          require("lgtm.files").render(state)
        end
        if not was_empty then
          require("lgtm.notifications").show(
            state.mode == "staged" and "No staged changes"
              or state.mode == "working" and "No working-tree changes"
              or "No branch changes"
          )
        end
        return
      end
      state.empty = false
      for _, candidate in ipairs(state.files) do
        if selected and candidate.path == selected.path and candidate.group == selected.group then
          file = candidate
          break
        end
      end
    end
    local visible_win = session.modified_win or session.original_win
    if not visible_win or not api.nvim_win_is_valid(visible_win) then
      return
    end
    state.tab = api.nvim_win_get_tabpage(visible_win)
    require("lgtm.services.state").tabs[state.tab] = state
    state.bind_panes = ready
    if state.native_explorer and session.panel.view and not state.panel_observer then
      local panel = session.panel.view
      local original = panel.on_data
      local function changed(data, changes)
        if original then
          original(data, changes)
        end
        if changes.list then
          -- CodeDiff changes refresh generations when a selection disappears. Its old
          -- completion callback can be discarded; panel data still publishes the empty list.
          vim.schedule(function()
            if not state.closed and state.tab == api.nvim_get_current_tabpage() then
              state.wait_ready()
            end
          end)
        end
      end
      state.panel_observer = { panel = panel, original = original, changed = changed }
      panel.on_data = changed
    end
    state.layout = session.layout
    state.panes = {
      old = { buf = session.original_bufnr, win = session.original_win },
      new = { buf = session.modified_bufnr, win = session.modified_win },
    }
    state.file = file
    state.views = {}
    state.source_generation = (state.source_generation or 0) + 1
    local source_generation = state.source_generation
    if state.native_explorer and session.modified_bufnr then
      require("lgtm.files").observe(state, session.modified_bufnr)
    end
    state.render = function()
      if state.closed or state.tab ~= api.nvim_get_current_tabpage() then
        return
      end
      require("lgtm.files").render(state)
      require("lgtm.threads").render_review(state, state.panes, file.path)
    end
    for side, pane in pairs(state.panes) do
      if pane.buf and api.nvim_buf_is_valid(pane.buf) then
        if vim.bo[pane.buf].buftype ~= "" then
          vim.bo[pane.buf].modifiable = false
        end
        state.owned_buffers = state.owned_buffers or {}
        state.owned_buffers[pane.buf] = true
        state.view_id = state.view_id or tostring(vim.uv.hrtime())
        vim.b[pane.buf].lgtm = {
          path = file.path,
          side = side,
          content_hash = file.contentHash,
          view_id = state.view_id,
        }
        require("lgtm.threads").attach(state, pane.buf, file.path, side)
        require("lgtm.files").attach(state, pane.buf)
        local function comment()
          local mode = api.nvim_get_mode().mode
          local range
          if mode == "v" or mode == "V" or mode == "\22" then
            local first, last = vim.fn.line("v"), api.nvim_win_get_cursor(0)[1]
            range = { math.min(first, last), math.max(first, last) }
            vim.cmd.normal({
              args = { api.nvim_replace_termcodes("<Esc>", true, false, true) },
              bang = true,
            })
          end
          vim.schedule(function()
            require("lgtm.review").comment("line", range)
          end)
        end
        require("lgtm.bindings").set(
          state,
          { "n", "x" },
          "c",
          comment,
          { buffer = pane.buf, desc = "Add review comment" }
        )
        require("lgtm.bindings").set(
          state,
          { "n", "x" },
          require("lgtm.config").options.prefix .. "c",
          vim.schedule_wrap(function()
            require("lgtm.review").comment("line")
          end),
          { buffer = pane.buf, desc = "Add review comment" }
        )
        if pane.win and api.nvim_win_is_valid(pane.win) then
          vim.wo[pane.win].winbar = "LGTM · " .. side .. " · " .. file.path
        end
      end
    end
    if not state.native_explorer and state.explorer_visible ~= false then
      require("lgtm.files").show(state, false)
    end
    state.render()
    if state.focus_side then
      local pane = state.panes[state.focus_side]
      if not pane.win then
        pane = state.panes.old.win and state.panes.old or state.panes.new
      end
      state.focus_side = nil
      if pane and pane.win and api.nvim_win_is_valid(pane.win) then
        api.nvim_set_current_win(pane.win)
      end
    end
    state.observe_sources = function(done)
      require("lgtm.services.sources").observe(state, file, state.views, done)
    end
    local remaining = 2
    for _, side in ipairs({ "old", "new" }) do
      local pane = state.panes[side]
      require("lgtm.services.sources").read(state, file, side, function(err, selected)
        if
          require("lgtm.services.state").current ~= state
          or state.tab ~= api.nvim_get_current_tabpage()
          or state.source_generation ~= source_generation
        then
          return
        end
        local view_source = vim.tbl_extend("force", selected or {}, {
          path = side == "old" and (file.previousPath or file.path) or file.path,
          logical_path = file.path,
          previous_path = file.previousPath,
          side = side,
          confirmed_rename = file.confirmedRename,
          comparison = state.comparison,
          unavailable = err and err.message,
        })
        if view_source.source and not err and pane.buf and api.nvim_buf_is_valid(pane.buf) then
          local _, mismatch =
            require("lgtm.services.sources").capture(view_source, pane.buf, "file")
          if mismatch then
            view_source.unavailable = mismatch.message
          end
        end
        state.views[side] = view_source
        if side == "new" then
          file.saved_stamp = selected and selected.saved_stamp or false
          file.index_oid = selected and selected.source and selected.source.blob_oid or false
        end
        remaining = remaining - 1
        if remaining == 0 then
          local old, new = state.views.old.source, state.views.new.source
          file.contentHash = vim.json.encode({ old = old or false, new = new or false })
          state.render()
          state.render_ready = true
          require("lgtm.services.files").validate_reviewed(state)
          state.observe_sources(function(observe_error)
            if observe_error then
              require("lgtm.notifications").error(observe_error)
            end
          end)
          if state.on_ready then
            state.on_ready()
          end
        end
      end)
    end
  end
  local deadline = vim.uv.hrtime() + 5e9
  local function wait_ready()
    if generation ~= state.render_generation then
      return
    end
    local session = state.tab and require("codediff.ui.lifecycle").get_session(state.tab)
    if
      session
      and (session.stored_diff_result or state.native_explorer and session.panel and not session.panel.data.current_selection)
      and (
        not session.refresh
        or (
          not session.refresh.loading
          and not session.refresh.running
          and not next(session.refresh.pending)
        )
      )
    then
      ready()
    elseif vim.uv.hrtime() < deadline then
      vim.defer_fn(wait_ready, 10)
    else
      require("lgtm.notifications").error("CodeDiff did not finish loading this file")
    end
  end
  state.wait_ready = function()
    deadline = vim.uv.hrtime() + 5e9
    wait_ready()
  end
  if native_config then
    state.explorer_group =
      api.nvim_create_augroup("LgtmNativeExplorer" .. tostring(vim.uv.hrtime()), { clear = true })
    api.nvim_create_autocmd("User", {
      group = state.explorer_group,
      pattern = "CodeDiffFileSelect",
      callback = function(event)
        if
          require("lgtm.services.state").current ~= state
          or (state.tab and event.data.tabpage ~= state.tab)
        then
          return
        end
        state.tab = event.data.tabpage
        local session = require("codediff.ui.lifecycle").get_session(state.tab)
        if session and session.panel and session.panel.data.status_result then
          M.sync_files(state, session.panel.data.status_result)
        end
        for _, selected in ipairs(state.files) do
          if selected.path == event.data.path then
            file = selected
            break
          end
        end
        state.render_ready = false
        state.source_generation = (state.source_generation or 0) + 1
        state.views = {}
        if state.render then
          state.render()
        end
        vim.schedule(state.wait_ready)
      end,
    })
  end
  if state.tab and api.nvim_tabpage_is_valid(state.tab) then
    api.nvim_set_current_tabpage(state.tab)
    view.update(state.tab, config, false)
    vim.schedule(wait_ready)
  else
    local result = view.create(config, vim.filetype.match({ filename = file.path }) or "", ready)
    if result then
      state.tab = api.nvim_win_get_tabpage(result.modified_win or result.original_win)
    end
  end
end

---@param state lgtm.ReviewState
function M.open_native(state)
  local comparison = vim.deepcopy(state.comparison)
  require("lgtm.git_metadata").install()
  local git = require("codediff.core.git")
  local base = state.base
  local head = state.mode == "working" and "WORKING"
    or state.mode == "staged" and ":0"
    or state.head
  local function loaded(err, status)
    vim.schedule(function()
      if
        state.closed
        or require("lgtm.services.state").current ~= state
        or not vim.deep_equal(comparison, state.comparison)
      then
        return
      end
      if err then
        require("lgtm.notifications").error(err)
        return
      end
      M.sync_files(state, status)
      if #state.files == 0 then
        require("lgtm.notifications").show(
          state.mode == "staged" and "No staged changes"
            or state.mode == "working" and "No working-tree changes"
            or "No branch changes"
        )
        return
      end
      state.native_explorer = true
      local paths = require("codediff.core.path")
      M.open(state, state.files[1], {
        git_root = state.git_root,
        original = paths.empty(),
        modified = paths.empty(),
        original_revision = base,
        modified_revision = head,
        panel = {
          name = "explorer",
          data = { status_result = status, source_revisions = { original = base, modified = head } },
        },
        layout = "side-by-side",
      })
    end)
  end
  if state.mode == "working" then
    git.get_diff_revision_with_line_stats(base, state.git_root, loaded)
  elseif state.mode == "staged" then
    git.get_diff_staged(base, state.git_root, loaded)
  else
    git.get_diff_revisions_with_line_stats(base, head, state.git_root, loaded)
  end
end

---@param state lgtm.ReviewState
function M.toggle(state)
  if not state or not state.tab then
    return
  end
  if require("codediff.ui.view.toggle").toggle(state.tab) then
    vim.schedule(function()
      if state.wait_ready then
        state.wait_ready()
      end
    end)
  end
end

---@param state lgtm.ReviewState
function M.suspend(state)
  state.source_generation = (state.source_generation or 0) + 1
  state.render_ready = false
  require("lgtm.bindings").restore(state)
  for buf in pairs(state.owned_buffers or {}) do
    if api.nvim_buf_is_valid(buf) then
      local tag = vim.b[buf].lgtm
      if tag and tag.view_id == state.view_id then
        vim.b[buf].lgtm = nil
        api.nvim_buf_clear_namespace(buf, require("lgtm.threads").namespace, 0, -1)
        api.nvim_buf_clear_namespace(buf, require("lgtm.threads").padding_namespace, 0, -1)
      end
    end
  end
end

---@param state lgtm.ReviewState
function M.release(state)
  local observer = state.panel_observer
  if observer and observer.panel.on_data == observer.changed then
    observer.panel.on_data = observer.original
  end
  state.panel_observer = nil
  M.suspend(state)
  state.closed = true
  state.render_generation = (state.render_generation or 0) + 1
  if state.tab then
    require("lgtm.services.state").tabs[state.tab] = nil
  end
  if state.explorer_group then
    pcall(api.nvim_del_augroup_by_id, state.explorer_group)
    state.explorer_group = nil
  end
end

---@param state lgtm.ReviewState
function M.close(state)
  M.release(state)
  if state.tab and api.nvim_tabpage_is_valid(state.tab) then
    pcall(api.nvim_set_current_tabpage, state.tab)
    if #api.nvim_list_tabpages() > 1 then
      vim.cmd("tabclose")
    end
  end
  if state.explorer and state.explorer.buf and api.nvim_buf_is_valid(state.explorer.buf) then
    pcall(api.nvim_buf_delete, state.explorer.buf, { force = true })
  end
  if state.snapshot_group then
    pcall(api.nvim_del_augroup_by_id, state.snapshot_group)
    state.snapshot_group = nil
  end
  if state.temp_dir then
    vim.fn.delete(state.temp_dir, "rf")
  end
end
return M
