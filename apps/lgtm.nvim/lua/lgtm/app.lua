-- State -----------------------------------------------------------------------

local M = {}
local Private = {}
local contexts = require("lgtm.services.state")
local notice = require("lgtm.ui.notifications")

-- Lifecycle -------------------------------------------------------------------

---@param state lgtm.ReviewState
function M.open(state, on_loaded)
  state.closed = false
  state.on_error = notice.error
  contexts.set(state)
  state.refresh = function(done)
    local comparison = vim.deepcopy(state.comparison)
    require("lgtm.services.session_service").refresh(state, function(err)
      if contexts.current ~= state then
        return
      end
      if done then
        done(err)
      elseif err then
        notice.error(err)
      end
      if not err and state.kind == "pr" and not vim.deep_equal(comparison, state.comparison) then
        vim.schedule(function()
          if contexts.current == state and not state.closed then
            M.refresh_view(state)
          end
        end)
      end
    end)
  end
  state.show_file = function(path, anchor, on_ready)
    if state.kind == "directory" and path:sub(1, 1) == "/" then
      local prefix = state.root .. "/"
      if path:sub(1, #prefix) == prefix then
        path = path:sub(#prefix + 1)
      end
    end
    local function ready()
      if anchor and anchor.kind == "line" then
        local pane = state.panes[anchor.side or "new"]
        if pane and vim.api.nvim_win_is_valid(pane.win) then
          vim.api.nvim_set_current_win(pane.win)
          pcall(vim.api.nvim_win_set_cursor, pane.win, { tonumber(anchor.start), 0 })
        end
      end
      state.on_ready = nil
      if on_ready then
        on_ready()
      end
    end
    if state.native_explorer then
      if state.tab and vim.api.nvim_tabpage_is_valid(state.tab) then
        vim.api.nvim_set_current_tabpage(state.tab)
      end
      local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
      for _, file in ipairs(state.files) do
        if file.path == path then
          -- The selected file already has its source state and cards. Selecting it
          -- again starts another asynchronous bind that can reset the requested view.
          if state.file == file and state.render_ready then
            ready()
            return
          end
          state.on_ready = (anchor or on_ready) and ready or nil
          panel.on_file_select(file.native_entry)
          return
        end
      end
    else
      if anchor and anchor.side == "old" and state.layout == "inline" then
        require("lgtm.features.diff_view").toggle(state)
      end
      for _, file in ipairs(state.files or {}) do
        if file.path == path then
          if not file.loaded then
            local pending = notice.show("Loading " .. path .. "…")
            require("lgtm.services.diff_service").get_file(state, file, function(err)
              if contexts.current ~= state then
                return
              end
              if err then
                notice.error(err, pending)
                return
              end
              state.show_file(path, anchor, on_ready)
              notice.show("File ready", nil, pending)
            end)
          else
            state.on_ready = (anchor or on_ready) and ready or nil
            require("lgtm.features.diff_view").open(state, file)
          end
          return
        end
      end
    end
    notice.error("File is outside this diff. Its discussion is retained in :Lgtm threads.")
  end
  if not state.snapshot then
    if not state.initializing and not state.load_error then
      notice.error(
        "Saved review has no retained source. Use :Lgtm threads or export, or start a new review."
      )
      state.refresh(on_loaded)
    end
    return
  end
  local function browse()
    if state.browsing then
      return
    end
    state.browsing = true
    if state.git_root and state.kind ~= "directory" then
      require("lgtm.features.diff_view").open_native(state)
    elseif not state.initializing and not state.load_error then
      require("lgtm.services.files_service").get_files(state, function(err)
        if contexts.current ~= state then
          return
        end
        if err then
          notice.error(err)
        else
          M.files()
        end
      end)
    else
      state.browsing = false
    end
  end
  browse()
  if not state.initializing and not state.load_error then
    state.refresh(on_loaded)
  end
end

-- Reuse the tab so an open composer keeps its buffer when a push changes the diff.

---@param state lgtm.ReviewState
function M.refresh_view(state)
  local comparison = vim.deepcopy(state.comparison)
  local function current()
    return contexts.current == state
      and not state.closed
      and vim.deep_equal(comparison, state.comparison)
  end
  local function open_current()
    if not current() then
      return
    end
    local composer = require("lgtm.ui.composer").last
    if composer and composer.win and vim.api.nvim_win_is_valid(composer.win) then
      -- Replacing a remote snapshot tab must not discard an open composer.
      vim.api.nvim_create_autocmd("WinClosed", {
        pattern = tostring(composer.win),
        once = true,
        callback = function()
          vim.schedule(open_current)
        end,
      })
      return
    end
    require("lgtm.features.diff_view").close(state)
    state.tab, state.browsing, state.native_explorer = nil, nil, nil
    state.file, state.empty = nil, nil
    require("lgtm.services.diff_service").clear(state)
    require("lgtm.services.files_service").set_files(state, {})
    M.open(state)
  end
  local session = state.tab and require("codediff.ui.lifecycle").get_session(state.tab)
  if state.native_explorer and session and session.panel then
    state.render_ready = false
    require("lgtm.services.files_service").get_files(state, function(err)
      if not current() then
        return
      end
      if err then
        notice.error(err)
        return
      end
      local refresh = require("codediff.ui.refresh")
      session.panel.data.source_revisions = { original = state.base, modified = state.head }
      refresh.request(state.tab, { full = true }, function()
        if not current() then
          return
        end
        refresh.select(
          state.tab,
          session.panel.data.current_selection,
          { force = true, no_jump = true }
        )
        state.wait_ready()
      end)
    end)
  else
    open_current()
  end
  notice.show("PR updated; pending comments retained")
end

function M.close()
  local state = contexts.current
  if state then
    require("lgtm.features.diff_view").close(state)
    contexts.set(contexts.tabs[vim.api.nvim_get_current_tabpage()])
  end
end

-- Methods ---------------------------------------------------------------------

function M.files()
  local state = contexts.current
  if not state then
    notice.error("Select a review first")
    return
  end
  if state.native_explorer then
    local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
    if panel and panel.winid and vim.api.nvim_win_is_valid(panel.winid) then
      vim.api.nvim_set_current_win(panel.winid)
    end
    return
  end
  if not state.file then
    for _, file in ipairs(state.files or {}) do
      if file.status ~= "binary" and file.status ~= "too_large" then
        state.show_file(file.path)
        return
      end
    end
    if #(state.files or {}) == 0 then
      notice.show(
        state.mode == "staged" and "No staged changes"
          or state.mode == "working" and "No working-tree changes"
          or "No changed files"
      )
    else
      notice.show("No supported text files in this review")
    end
  else
    require("lgtm.features.files_pane").show(state, true)
  end
end

function M.sync()
  local state = contexts.current
  if not state or state.initializing or state.load_error then
    notice.error("Open a review and wait for Review ready before synchronizing drafts")
    return
  end
  if state.kind ~= "pr" then
    notice.show("Local review drafts are already saved")
    return
  end
  local pending = notice.show("Synchronizing review drafts…")
  require("lgtm.services.threads_service").sync_drafts(state, function(err)
    if err then
      notice.error(err, pending)
    else
      notice.show(
        "Drafts synchronized; general comments remain local until submission",
        nil,
        pending
      )
    end
    if not state.render_ready then
      M.refresh_view(state)
    end
  end)
end

function M.comment(kind, range)
  local state = contexts.current
  if not state then
    notice.show("Select a review first")
    return
  end
  local unavailable = contexts.get_readiness_error(state)
  if unavailable then
    notice.error(unavailable)
    return
  end
  kind = kind or "line"
  local location = vim.b.lgtm or {}
  if kind ~= "general" and (not state.file or not location.side) then
    notice.error("Open a review file first")
    return
  end
  local first, last = vim.api.nvim_win_get_cursor(0)[1], vim.api.nvim_win_get_cursor(0)[1]
  if vim.fn.mode():match("[vV\22]") then
    first, last = vim.fn.line("v"), vim.fn.line(".")
    first, last = math.min(first, last), math.max(first, last)
  end
  if range then
    first, last = range[1], range[2]
  end
  local generation = state.render_generation
  local function open_composer(capture_error, anchor)
    if contexts.current ~= state or generation ~= state.render_generation then
      return
    end
    if capture_error then
      notice.error(capture_error)
      return
    end
    local input = require("lgtm.services.threads_service").create_draft(state, anchor)
    require("lgtm.ui.composer").open("Review comment", "", function(body, done)
      local saving = notice.show("Saving draft…")
      require("lgtm.services.threads_service").save(state, input, body, function(save_error, result)
        if save_error then
          notice.error(save_error, saving)
        elseif result.syncError and result.syncError ~= "" then
          notice.error(
            "Draft saved locally; GitHub sync failed: "
              .. result.syncError
              .. ". Use :Lgtm sync to recover.",
            saving
          )
        else
          notice.show(
            result.comment
                and result.comment.forgeCommentId
                and result.comment.forgeCommentId ~= ""
                and "Draft saved on GitHub"
              or "Draft saved locally",
            nil,
            saving
          )
        end
        done(save_error)
        if not save_error and not state.render_ready then
          M.refresh_view(state)
        end
      end)
    end)
  end
  open_composer(nil, {
    kind = kind,
    snapshotId = state.snapshot.id,
    path = kind ~= "general"
        and (location.side == "old" and (state.file.previousPath or state.file.path) or state.file.path)
      or nil,
    side = kind == "line" and location.side or nil,
    start = kind == "line" and first or nil,
    ["end"] = kind == "line" and last or nil,
  })
end

function M.comment_action(action)
  local buf = vim.api.nvim_get_current_buf()
  local state = contexts.current
  local location = vim.b[buf].lgtm
  local function perform(selected, path, side)
    local threads = require("lgtm.features.annotation_overlay")
    local matches = threads.at(selected, path, side, vim.api.nvim_win_get_cursor(0)[1])
    if #matches == 0 then
      notice.show("No comment at cursor")
      return
    end
    require("lgtm.features.threads_pane").open_selected(
      selected,
      matches,
      nil,
      #matches == 1 and function()
        vim.api.nvim_input(action)
      end or nil
    )
    if #matches > 1 then
      notice.show("Select a comment, then " .. action)
    end
  end
  if location and state then
    perform(state, location.path, location.side)
  else
    notice.error("Open a review source pane first")
  end
end

function M.refresh()
  local state = contexts.current
  if state and state.load_error and state.retry_load then
    state.retry_load()
    return
  end
  if not state or state.initializing or state.load_error then
    return
  end
  local revision = state.snapshot.id
  local pending = notice.show("Refreshing review…")
  require("lgtm.services.session_service").refresh_comparison(state, function(err)
    if contexts.current ~= state then
      return
    end
    if err then
      notice.error(err, pending)
      return
    end
    if revision == state.snapshot.id then
      require("lgtm.services.files_service").get_reviewed(state)
    end
    local function content()
      state.refresh(function(error)
        if error then
          notice.error(error, pending)
          return
        end
        if state.native_explorer then
          state.render_ready = false
          require("codediff.ui.refresh").request(state.tab, { full = true })
          state.wait_ready()
        else
          local path = state.file and state.file.path
          require("lgtm.services.files_service").get_files(state, function(load_error)
            if load_error then
              notice.error(load_error)
            elseif path then
              state.show_file(path)
            else
              M.files()
            end
          end)
        end
        notice.show("Review refreshed", nil, pending)
      end)
    end
    if revision ~= state.snapshot.id then
      state.refresh(function(refresh_error)
        if refresh_error then
          notice.error(refresh_error, pending)
          return
        end
        M.refresh_view(state)
        notice.show("PR updated; pending comments retained", nil, pending)
      end)
    else
      content()
    end
  end)
end

function M.save()
  local state = contexts.current
  if not state or state.initializing or state.load_error then
    return
  end
  state.refresh(function(err)
    if err then
      notice.error(err)
    else
      notice.show("Review saved (drafts persist after every change)")
    end
  end)
end

function M.complete()
  local state = contexts.current
  if not state then
    return
  end
  if state.kind == "pr" then
    notice.error("Use :Lgtm submit for GitHub reviews")
    return
  end
  require("lgtm.services.session_service").close(state, function(err)
    if err then
      notice.error(err)
    else
      notice.show("Local review completed")
    end
  end)
end

function M.submit()
  local state = contexts.current
  if not state then
    return
  end
  local service = require("lgtm.services.session_service")
  local unavailable = service.get_submission_error(state)
  if unavailable then
    notice.error(unavailable.message)
    return
  end
  if state.review.submissionStatus == "failed" then
    local pending = notice.show("Checking rejected submission…")
    service.recover(state, function(err, result)
      if not err and result.editable then
        notice.show(result.message, nil, pending)
        M.submit()
      else
        Private.present_submission(err, result, pending, function() end)
      end
    end)
    return
  end
  require("lgtm.ui.picker").select(
    "Submit review",
    { "comment", "approve", "request_changes" },
    tostring,
    function(outcome)
      local draft = service.get_draft(state)
      require("lgtm.ui.composer").open(
        "Final review body (optional)",
        draft.body,
        function(body, done)
          local pending = notice.show("Submitting review…")
          service.submit(state, draft, outcome, body, function(err, result)
            Private.present_submission(err, result, pending, done)
          end)
        end,
        { action = "submit review" }
      )
    end
  )
end

function M.reconcile()
  local state = contexts.current
  if not state then
    return
  end
  local service = require("lgtm.services.session_service")
  local unavailable = service.get_reconcile_error(state)
  if unavailable then
    notice.error(unavailable.message)
    return
  end
  local pending = notice.show("Checking submission status…")
  service.reconcile(state, function(err, result)
    Private.present_submission(err, result, pending, function() end)
  end)
end

function M.reviewed(path)
  local state = contexts.current
  if not state or not state.file then
    return
  end
  if state.native_explorer and not path then
    local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
    if panel and vim.api.nvim_get_current_buf() == panel.bufnr then
      local node = panel.tree:get_node()
      if
        not node
        or not node.data
        or node.data.type == "group"
        or node.data.type == "directory"
      then
        return
      end
      path = node.data.path
    end
  end
  path = path or state.file.path
  for _, file in ipairs(state.files or {}) do
    if file.path == path then
      local origin = vim.api.nvim_get_current_win()
      local cursor = vim.api.nvim_win_get_cursor(origin)
      local function toggle()
        require("lgtm.features.files_pane").toggle_reviewed(state, file)
        if vim.api.nvim_win_is_valid(origin) then
          vim.api.nvim_set_current_win(origin)
          pcall(vim.api.nvim_win_set_cursor, origin, cursor)
        end
      end
      if state.file.path ~= path or not state.render_ready then
        -- Read the selected revision before recording its content identity.
        state.show_file(path, nil, toggle)
      else
        toggle()
      end
      return
    end
  end
end

function M.export(scope)
  local function show(state)
    require("lgtm.services.session_service").get_export(state, function(err, markdown)
      if err then
        notice.error(err)
      else
        require("lgtm.ui.export").open(markdown, scope or "review")
      end
    end)
  end
  if contexts.current then
    show(contexts.current)
  else
    notice.error("Select a review first")
  end
end

-- Private ---------------------------------------------------------------------

function Private.present_submission(err, result, pending, done)
  done(err)
  if err then
    notice.error(result.message, pending)
  else
    notice.show(result.message, nil, pending)
  end
  if result.warning then
    notice.error(result.warning)
  end
end

return M
