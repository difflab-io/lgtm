local M = { submission_drafts = {} }
local rpc, contexts, notice =
  require("lgtm.rpc"), require("lgtm.context"), require("lgtm.notifications")
-- Directory and remote fallbacks enumerate metadata, then fetch only the selected file.
function M.load_diff(state, callback)
  local previous = {}
  for _, file in ipairs(state.files or {}) do
    previous[file.path] = file
  end
  rpc.call(
    "GetDiff",
    { target = state.target, comparison = state.comparison },
    function(err, response)
      if err then
        callback(err)
        return
      end
      state.diff = response.diff
      state.root = response.diff.root
      local files = {}
      local function page(token)
        rpc.call(
          "GetFiles",
          { diff = state.diff, pageToken = token, pageSize = 100 },
          function(error, result)
            if error then
              callback(error)
              return
            end
            for _, file in ipairs(result.files or {}) do
              local saved = previous[file.path]
              files[#files + 1] = saved and saved.identity == file.identity and saved or file
            end
            if result.nextPageToken and result.nextPageToken ~= "" then
              page(result.nextPageToken)
            else
              state.files = files
              callback()
            end
          end
        )
      end
      page(nil)
    end
  )
end

function M.open(state, on_loaded)
  state.closed = false
  contexts.set(state)
  state.refresh = function(done)
    local comparison = vim.deepcopy(state.comparison)
    contexts.refresh(state, function(err)
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
            M.reopen_pr(state)
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
          state.on_ready = (anchor or on_ready) and ready or nil
          panel.on_file_select(file.native_entry)
          return
        end
      end
    else
      if anchor and anchor.side == "old" and state.layout == "inline" then
        require("lgtm.codediff").toggle(state)
      end
      for _, file in ipairs(state.files or {}) do
        if file.path == path then
          if not file.loaded then
            local pending = notice.show("Loading " .. path .. "…")
            rpc.latest(
              "review-file",
              "GetFile",
              { diff = state.diff, path = path },
              function(err, result)
                if contexts.current ~= state then
                  return
                end
                if err then
                  notice.error(err, pending)
                  return
                end
                file.sides = { old = result.oldSide, new = result.newSide }
                file.content = result.newSide and result.newSide.content or ""
                file.baseContent = result.oldSide and result.oldSide.content or ""
                file.contentHash = file.identity
                file.loaded = true
                state.show_file(path, anchor, on_ready)
                notice.show("File ready", nil, pending)
              end
            )
          else
            state.on_ready = (anchor or on_ready) and ready or nil
            require("lgtm.codediff").open(state, file)
          end
          return
        end
      end
    end
    notice.error("File is outside this diff. Its discussion is retained in :Lgtm threads.")
  end
  if (state.kind == "git" or state.kind == "pr") and not state.comparison then
    notice.error(
      "Saved review has no recorded comparison. Use :Lgtm threads or export; select an explicit comparison for a new review."
    )
    if not state.initializing and not state.load_error then
      state.refresh(on_loaded)
    end
    return
  end
  local function browse()
    if state.browsing then
      return
    end
    state.browsing = true
    if state.git_root then
      require("lgtm.codediff").open_native(state)
    elseif not state.initializing and not state.load_error then
      M.load_diff(state, function(err)
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
  if state.kind == "git" then
    state.git_root = state.root
  end
  if state.kind == "pr" and not state.git_checked then
    local comparison = vim.deepcopy(state.comparison)
    state.git_checked, state.git_pending = true, true
    vim.system({
      "git",
      "-C",
      vim.fn.getcwd(),
      "rev-parse",
      "--show-toplevel",
      state.base .. "^{commit}",
      state.head .. "^{commit}",
    }, { text = true }, function(result)
      vim.schedule(function()
        if
          state.closed
          or contexts.current ~= state
          or not vim.deep_equal(comparison, state.comparison)
        then
          return
        end
        state.git_pending = false
        if result.code == 0 then
          state.git_root = vim.split(vim.trim(result.stdout), "\n", { plain = true })[1]
        end
        browse()
      end)
    end)
  else
    browse()
  end
  if not state.initializing and not state.load_error then
    state.refresh(on_loaded)
  end
end

-- Reuse the tab so an open composer keeps its buffer when a push changes the diff.
function M.reopen_pr(state)
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
    local composer = require("lgtm.composer").last
    if composer and vim.api.nvim_win_is_valid(composer.win) then
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
    require("lgtm.codediff").close(state)
    state.tab, state.browsing, state.native_explorer, state.git_checked, state.git_pending, state.git_root =
      nil, nil, nil, nil, nil, nil
    state.file, state.files, state.diff, state.empty = nil, {}, nil, nil
    M.open(state)
  end
  local session = state.tab and require("codediff.ui.lifecycle").get_session(state.tab)
  if state.native_explorer and session and session.panel then
    state.render_ready = false
    vim.system({
      "git",
      "-C",
      state.git_root,
      "rev-parse",
      state.base .. "^{commit}",
      state.head .. "^{commit}",
    }, { text = true }, function(result)
      vim.schedule(function()
        if not current() then
          return
        end
        if result.code ~= 0 then
          open_current()
          return
        end
        local refresh = require("codediff.ui.refresh")
        session.panel.data.source_revisions = { original = state.base, modified = state.head }
        refresh.request(state.tab, { full = true }, function()
          if not current() then
            return
          end
          -- CodeDiff selects its current regular, added, removed, or empty layout.
          refresh.select(
            state.tab,
            session.panel.data.current_selection,
            { force = true, no_jump = true }
          )
          state.wait_ready()
        end)
      end)
    end)
  else
    open_current()
  end
  notice.show("PR updated; pending comments retained")
end

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
    require("lgtm.files").show(state, true)
  end
end

function M.directory(path)
  local function start(selected)
    local root = vim.fn.fnamemodify(selected, ":p"):gsub("/$", "")
    contexts.start(
      { directory = { path = root } },
      { directory = vim.empty_dict() },
      root,
      function(err, state)
        if err then
          notice.error(err)
        else
          M.open(state)
        end
      end
    )
  end
  if path then
    start(path)
  else
    require("lgtm.picker").input(
      "Review directory (includes ignored files): ",
      vim.fn.getcwd(),
      start
    )
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
  local drafts = {}
  for _, c in ipairs((state.conversation or {}).comments or {}) do
    if
      c.originSessionId == state.review.id
      and c.author
      and (c.ownerId ~= "" and c.ownerId or c.author.id) == rpc.identity
      and c.visibility == "draft"
    then
      drafts[#drafts + 1] = c
    end
  end
  table.sort(drafts, function(a, b)
    return tonumber(a.position or 0) < tonumber(b.position or 0)
  end)
  local pending = notice.show("Synchronizing review drafts…")
  local index = 0
  local function next_draft()
    index = index + 1
    if not drafts[index] then
      state.refresh(function(err)
        if err then
          notice.error(err, pending)
        else
          notice.show(
            "Drafts synchronized; general comments remain local until submission",
            nil,
            pending
          )
        end
      end)
      return
    end
    rpc.call(
      "SyncDraftComment",
      { reviewId = state.review.id, commentId = drafts[index].id },
      function(err)
        if err then
          notice.error(err, pending)
          state.refresh()
          return
        end
        next_draft()
      end
    )
  end
  next_draft()
end

function M.select()
  require("lgtm.picker").select(
    "Review",
    { "working", "staged", "branch", "PR" },
    tostring,
    function(kind)
      if kind == "PR" then
        require("lgtm.sessions").prs()
      else
        M.branch(kind)
      end
    end
  )
end

function M.branch(requested_mode, default_base)
  local root = vim.fn.getcwd()
  vim.system({
    "git",
    "-C",
    root,
    "rev-parse",
    "--show-toplevel",
    "--absolute-git-dir",
    "--symbolic-full-name",
    "HEAD",
  }, { text = true }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        notice.error("Not a Git repository; use :Lgtm directory")
        return
      end
      local identity = vim.split(vim.trim(result.stdout), "\n", { plain = true })
      root = identity[1]
      if not identity[3] or not identity[3]:match("^refs/heads/") then
        notice.error("Check out a local branch before Git review, or use :Lgtm directory")
        return
      end
      local function select_mode(mode)
        local function start(base, head)
          vim.system(
            { "git", "-C", root, "rev-parse", base, head },
            { text = true },
            function(resolved)
              vim.schedule(function()
                if resolved.code ~= 0 then
                  notice.error(resolved.stderr)
                  return
                end
                local refs = vim.split(vim.trim(resolved.stdout), "\n", { plain = true })
                local comparison = mode == "branch"
                    and { branch = { baseCommit = refs[1], headCommit = refs[2] } }
                  or { [mode] = { baseCommit = refs[1] } }
                contexts.start(
                  { localGit = { path = root, branch = identity[3] } },
                  comparison,
                  root,
                  function(err, state)
                    if err then
                      notice.error(err)
                    else
                      M.open(state)
                    end
                  end
                )
              end)
            end
          )
        end
        if mode == "branch" then
          require("lgtm.picker").input("Base branch: ", "main", function(base_branch)
            vim.system(
              { "git", "-C", root, "merge-base", base_branch, "HEAD" },
              { text = true },
              function(result)
                vim.schedule(function()
                  if result.code ~= 0 then
                    notice.error("Cannot find a common ancestor with " .. base_branch)
                    return
                  end
                  start(vim.trim(result.stdout), "HEAD")
                end)
              end
            )
          end)
        else
          start("HEAD", "HEAD")
        end
      end
      if requested_mode then
        select_mode(requested_mode)
      else
        require("lgtm.picker").select(
          "Review content",
          { "working", "staged" },
          tostring,
          select_mode
        )
      end
    end)
  end)
end

function M.comment(kind, range)
  local state = contexts.current
  if not state then
    notice.show("Select a review first, or use :Lgtm note")
    return
  end
  local unavailable = contexts.readiness_error(state)
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
  local buf = vim.api.nvim_get_current_buf()
  local first, last = vim.api.nvim_win_get_cursor(0)[1], vim.api.nvim_win_get_cursor(0)[1]
  if vim.fn.mode():match("[vV\22]") then
    first, last = vim.fn.line("v"), vim.fn.line(".")
    first, last = math.min(first, last), math.max(first, last)
  end
  if range then
    first, last = range[1], range[2]
  end
  local generation = state.render_generation
  local function compose(err, selected)
    if contexts.current ~= state or generation ~= state.render_generation then
      return
    end
    if err then
      notice.error(err)
      return
    end
    local view = selected
      and vim.tbl_extend("force", selected, {
        path = location.side == "old" and (state.file.previousPath or state.file.path)
          or state.file.path,
        side = location.side,
      })
    local anchor, capture_error = require("lgtm.sources").capture(view, buf, kind, first, last)
    if not anchor then
      notice.error(capture_error)
      return
    end
    local input = {
      scope = state.scope,
      expectedVersion = state.version,
      requestKey = contexts.request_key(),
      anchor = anchor,
    }
    require("lgtm.composer").open("Review comment", "", function(body, done)
      -- Capture the latest review version on first save; an open PR composer can
      -- retain verified source bytes across an explicit comparison refresh.
      if not input.body then
        input.expectedVersion = state.version
      end
      -- Keep the exact source, version, and operation key when an uncertain save is retried.
      if input.body and input.body ~= body then
        input.requestKey = contexts.request_key()
      end
      input.body = body
      local saving = notice.show("Saving draft…")
      rpc.call("AddComment", input, function(error, result)
        if error then
          notice.error(error, saving)
          done(error)
          return
        end
        state.refresh(function(refresh_error)
          if result.syncError and result.syncError ~= "" then
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
          done(refresh_error)
        end)
      end)
    end)
  end
  if kind == "general" then
    compose()
  else
    require("lgtm.sources").read(state, state.file, location.side, compose)
  end
end

function M.comment_action(action)
  local buf = vim.api.nvim_get_current_buf()
  local state = contexts.current
  local location = vim.b[buf].lgtm
  local function perform(selected, path, side)
    local threads = require("lgtm.threads")
    local matches = threads.at(selected, path, side, vim.api.nvim_win_get_cursor(0)[1])
    if #matches == 0 then
      notice.show("No comment at cursor")
      return
    end
    local popup = threads.open(selected, matches)
    if #matches == 1 then
      vim.api.nvim_input(action)
    else
      notice.show("Select a comment, then " .. action)
    end
  end
  if location and state then
    perform(state, location.path, location.side)
  else
    require("lgtm.notes").state(buf, function(err, notes)
      if err then
        notice.error(err)
      else
        perform(notes, notes.path, "new")
      end
    end)
  end
end

function M.threads()
  local state = contexts.current
  if not state then
    notice.error("Select a review first")
    return
  end
  require("lgtm.threads").open(state, (state.conversation or {}).threads or {}, true)
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
  if state.pending_location then
    require("lgtm.locations").retry(state, function(err)
      if err then
        notice.error(err)
      end
    end)
    return
  end
  local comparison = vim.deepcopy(state.comparison)
  local pending = notice.show("Refreshing review…")
  rpc.call(
    "RefreshReview",
    { scope = state.scope, expectedVersion = state.version, requestKey = contexts.request_key() },
    function(err, result)
      if contexts.current ~= state then
        return
      end
      if err then
        notice.error(err, pending)
        return
      end
      if result.review then
        contexts.apply(state, result.review)
        if vim.deep_equal(comparison, state.comparison) then
          require("lgtm.files").load_reviewed(state)
        end
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
            M.load_diff(state, function(load_error)
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
      if state.kind == "pr" and not vim.deep_equal(comparison, state.comparison) then
        state.refresh(function(refresh_error)
          if refresh_error then
            notice.error(refresh_error, pending)
            return
          end
          M.reopen_pr(state)
          notice.show("PR updated; pending comments retained", nil, pending)
        end)
      else
        content()
      end
    end
  )
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

function M.close()
  local state = contexts.current
  if state then
    require("lgtm.codediff").close(state)
    contexts.set(contexts.tabs[vim.api.nvim_get_current_tabpage()])
  end
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
  rpc.call("CompleteReview", {
    reviewId = state.review.id,
    expectedVersion = state.version,
    requestKey = contexts.request_key(),
  }, function(err, result)
    if err then
      notice.error(err)
    else
      contexts.apply(state, result.review)
      notice.show("Local review completed")
    end
  end)
end

function M.submission_result(state, err, result, pending, done)
  if err then
    local message = (
      err.uncertain and "Submission response lost. Use :Lgtm reconcile before retrying. "
      or "Submission failed. "
    ) .. (err.message or "Unknown error")
    local draft = M.submission_drafts[state.review.id]
    if draft then
      draft.uncertain = err.uncertain
    end
    -- Learn durable intent state without observing GitHub again or repeating a write.
    state.refresh(function()
      done(err)
      notice.error(message, pending)
    end)
    return
  end
  contexts.apply(state, result.review)
  local status = result.review.submissionStatus
  if status == "confirmed" then
    M.submission_drafts[state.review.id] = nil
    -- Submission already persisted the published visibility. Read local data;
    -- do not leave draft cards on screen or start another provider observation.
    contexts.conversation(state, function(read_error)
      if not read_error and state.render then
        state.render()
      end
      notice.show("Review submitted", nil, pending)
      done()
      if read_error then
        notice.error(
          "Review published; reload comments with :Lgtm refresh. " .. (read_error.message or "")
        )
      end
    end)
  elseif status == "unsubmitted" then
    local draft = M.submission_drafts[state.review.id]
    if draft then
      draft.uncertain, draft.fingerprint = false, nil
    end
    local message = result.review.submissionError or ""
    local error = {
      message = "Review is editable; drafts and final text are saved. Choose an outcome with :Lgtm submit. "
        .. message,
    }
    state.refresh(function()
      done(error)
      notice.error(error, pending)
    end)
  elseif
    status == "uncertain"
    or status == "pending"
    or status == "posting"
    or status == "sending"
    or status == "reconciling"
  then
    local error = { message = "Submission " .. status .. "; use :Lgtm reconcile before retrying" }
    notice.error(error, pending)
    done(error)
  else
    local error = {
      message = (result.review.submissionError or ("Submission " .. tostring(status)))
        .. ". Use :Lgtm reconcile to inspect/recover.",
    }
    notice.error(error, pending)
    done(error)
  end
end

local function can_recover_rejected(state)
  if
    state.review.submissionStatus == "failed"
    and not vim.list_contains(rpc.capabilities or {}, "rejected_submission_recovery")
  then
    notice.error(
      "Update the LGTM CLI/backend before recovering this rejected submission. Your drafts are preserved."
    )
    return false
  end
  return true
end

function M.submit()
  local unavailable = contexts.current and contexts.readiness_error(contexts.current)
  if unavailable then
    notice.error(unavailable)
    return
  end
  local state = contexts.current
  if not state then
    return
  end
  local retained = M.submission_drafts[state.review.id]
  if retained and retained.uncertain and state.review.submissionStatus ~= "failed" then
    notice.error("Submission response was lost. Use :Lgtm reconcile before retrying.")
    return
  end
  if (state.review.submissionStatus or ""):match("^draft_") then
    notice.error("Draft synchronization needs recovery. Use :Lgtm sync before submitting.")
    return
  end
  if
    state.review.submissionStatus == "uncertain"
    or state.review.submissionStatus == "pending"
    or state.review.submissionStatus == "posting"
    or state.review.submissionStatus == "sending"
    or state.review.submissionStatus == "reconciling"
  then
    notice.error("Submission is pending or uncertain. Use :Lgtm reconcile before retrying.")
    return
  end
  if state.review.submissionStatus == "failed" then
    if not can_recover_rejected(state) then
      return
    end
    local pending = notice.show("Checking rejected submission…")
    rpc.call("ReconcileSubmission", { reviewId = state.review.id }, function(err, result)
      if not err and result.review.submissionStatus == "unsubmitted" then
        contexts.apply(state, result.review)
        local draft = M.submission_drafts[state.review.id]
        if draft then
          draft.uncertain, draft.fingerprint = false, nil
        end
        notice.show("Drafts recovered; choose a review outcome", nil, pending)
        state.refresh(function(refresh_err)
          if refresh_err then
            notice.error(refresh_err)
          else
            M.submit()
          end
        end)
      else
        M.submission_result(state, err, result, pending, function() end)
      end
    end)
    return
  end
  if state.kind ~= "pr" then
    notice.error("Local reviews cannot be submitted to GitHub; use :Lgtm complete")
    return
  end
  require("lgtm.picker").select(
    "Submit review",
    { "comment", "approve", "request_changes" },
    tostring,
    function(outcome)
      local draft = M.submission_drafts[state.review.id]
        or { body = state.review.body or "", requestKey = contexts.request_key() }
      M.submission_drafts[state.review.id] = draft
      require("lgtm.composer").open("Final review body (optional)", draft.body, function(body, done)
        draft.body = body
        draft.outcome = outcome
        local pending = notice.show("Submitting review…")
        local input = {
          reviewId = state.review.id,
          outcome = outcome,
          body = body,
          expectedHead = state.head,
          expectedBase = state.base,
          expectedVersion = state.version,
        }
        local fingerprint = vim.json.encode(input)
        if draft.fingerprint ~= fingerprint then
          draft.requestKey, draft.fingerprint = contexts.request_key(), fingerprint
        end
        input.requestKey = draft.requestKey
        rpc.call("SubmitReview", input, function(err, result)
          M.submission_result(state, err, result, pending, done)
        end)
      end, { action = "submit review" })
    end
  )
end

function M.reconcile()
  local state = contexts.current
  if not state then
    return
  end
  local unavailable = contexts.readiness_error(state)
  if unavailable then
    notice.error(unavailable)
    return
  end
  if not can_recover_rejected(state) then
    return
  end
  local pending = notice.show("Checking submission status…")
  rpc.call("ReconcileSubmission", { reviewId = state.review.id }, function(err, result)
    M.submission_result(state, err, result, pending, function() end)
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
        require("lgtm.files").toggle_reviewed(state, file)
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
return M
