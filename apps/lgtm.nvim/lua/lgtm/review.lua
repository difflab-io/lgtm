local M = { submission_drafts = {} }
local rpc, contexts, notice =
  require("lgtm.rpc"), require("lgtm.context"), require("lgtm.notifications")
function M.open(state)
  contexts.set(state)
  state.refresh = function(done)
    local generation = contexts.generation
    rpc.call(
      "GetConversation",
      { contextId = state.context.id, includeDrafts = true },
      function(err, result)
        if generation ~= contexts.generation then
          return
        end
        if not err then
          state.conversation = result
          if state.render then
            state.render()
          end
        end
        if done then
          done(err)
        elseif err then
          notice.error(err)
        end
      end
    )
  end
  state.show_file = function(path, anchor)
    if anchor and anchor.side == "old" and state.layout == "inline" then
      require("lgtm.codediff").toggle(state)
      vim.schedule(function()
        state.show_file(path, anchor)
      end)
      return
    end
    for _, file in ipairs(state.files or {}) do
      if file.path == path then
        if file.status == "binary" or file.status == "too_large" then
          notice.error("Unsupported review content: " .. path .. " (" .. file.status .. ")")
          return
        end
        state.on_ready = anchor
            and function()
              local pane = state.panes[anchor.side or "new"]
              if pane then
                vim.api.nvim_set_current_win(pane.win)
                pcall(vim.api.nvim_win_set_cursor, pane.win, { tonumber(anchor.start or 1), 0 })
              end
              state.on_ready = nil
            end
          or nil
        require("lgtm.codediff").open(state, file)
        return
      end
    end
    notice.error(
      "File no longer available: " .. path .. ". Its conversation is retained in :Lgtm threads."
    )
  end
  state.refresh(function(err)
    if err then
      notice.error(err)
      return
    end
    rpc.call("GetWorkspace", {
      contextId = state.context.id,
      includeIgnored = true,
      base = state.session.base,
      head = state.session.head,
      mode = state.mode or (state.context.key.kind == "notes" and "directory" or "working"),
    }, function(error, workspace)
      if contexts.current ~= state then
        return
      end
      if error then
        notice.error(error)
        return
      end
      state.root = workspace.root
      state.files = workspace.files or {}
      M.files()
    end)
  end)
end

function M.files()
  local state = contexts.current
  if not state then
    notice.error("Select a review first")
    return
  end
  if not state.file then
    for _, file in ipairs(state.files or {}) do
      if file.status ~= "binary" and file.status ~= "too_large" then
        state.show_file(file.path)
        return
      end
    end
    notice.show("No supported review files")
  else
    require("lgtm.files").show(state, true)
  end
end

function M.directory(path)
  local function start(selected)
    contexts.notes(selected, function(err, state)
      if err then
        notice.error(err)
      else
        M.open(state)
      end
    end)
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

function M.select()
  require("lgtm.picker").select("Review", { "working", "staged", "PR" }, tostring, function(kind)
    if kind == "PR" then
      require("lgtm.sessions").prs()
    else
      M.branch(kind)
    end
  end)
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
                contexts.ensure(
                  { kind = "branch", gitDir = identity[2], branch = identity[3] },
                  root,
                  function(err, state)
                    if err then
                      notice.error(err)
                    else
                      state.mode = mode
                      M.open(state)
                    end
                  end,
                  { base = refs[1], head = refs[2] }
                )
              end)
            end
          )
        end
        if mode == "revisions" then
          require("lgtm.picker").input("Base revision: ", default_base or "HEAD~1", function(base)
            require("lgtm.picker").input("Head revision: ", "HEAD", function(head)
              start(base, head)
            end)
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
    notice.error("Select a review first, or use :Lgtm note")
    return
  end
  local location = vim.b.lgtm or {}
  local anchor = { kind = kind or "line", path = location.path or (state.file and state.file.path) }
  if anchor.kind == "general" then
    anchor.path = nil
  elseif not anchor.path then
    notice.error("Open a review file first")
    return
  end
  if anchor.kind == "line" then
    local first, last = vim.api.nvim_win_get_cursor(0)[1], vim.api.nvim_win_get_cursor(0)[1]
    if vim.fn.mode():match("[vV\22]") then
      first, last = vim.fn.line("v"), vim.fn.line(".")
      first, last = math.min(first, last), math.max(first, last)
    end
    if range then
      first, last = range[1], range[2]
    end
    anchor.start = first
    anchor["end"] = last
    anchor.side = location.side or "new"
    anchor.commit = anchor.side == "old" and state.session.base or state.session.head
    if not anchor.commit or anchor.commit == "" then
      anchor.contentHash = location.content_hash
    end
  end
  require("lgtm.composer").open("Review comment", "", function(body, done)
    rpc.call(
      "AddComment",
      { sessionId = state.session.id, anchor = anchor, body = body },
      function(err)
        if err then
          done(err)
          return
        end
        state.refresh(function(refresh_err)
          if not refresh_err then
            notice.show("Draft saved")
          end
          done(refresh_err)
        end)
      end
    )
  end)
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
    vim.api.nvim_win_set_cursor(popup.win, { 2, 0 })
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
  if not state then
    return
  end
  local pending = notice.show("Refreshing review…")
  local function content()
    rpc.call("GetWorkspace", {
      contextId = state.context.id,
      includeIgnored = true,
      base = state.session.base,
      head = state.session.head,
      mode = state.mode or (state.context.key.kind == "notes" and "directory" or "working"),
    }, function(err, result)
      if contexts.current ~= state then
        return
      end
      if err then
        notice.error(err, pending)
        return
      end
      state.files = result.files or {}
      state.root = result.root
      state.refresh(function(error)
        if error then
          notice.error(error, pending)
          return
        end
        if state.file then
          state.show_file(state.file.path)
        end
        notice.show("Review refreshed", nil, pending)
      end)
    end)
  end
  if state.context.key.kind == "forge_backed" then
    rpc.call("RefreshContext", { contextId = state.context.id }, function(err, result)
      if contexts.current ~= state then
        return
      end
      if err then
        notice.error(err, pending)
        return
      end
      if result.context.refreshedHead ~= state.session.head then
        notice.error("PR head changed. Start a fresh review before adding more comments.", pending)
        return
      end
      state.context = result.context
      content()
    end)
  else
    content()
  end
end

function M.save()
  local state = contexts.current
  if not state then
    return
  end
  rpc.call("GetSession", { sessionId = state.session.id }, function(err, result)
    if err then
      notice.error(err)
    else
      state.session = result.session
      notice.show("Review saved (drafts persist after every change)")
    end
  end)
end

function M.close()
  local state = contexts.current
  if state then
    require("lgtm.codediff").close(state)
    contexts.set(nil)
  end
end

function M.complete()
  local state = contexts.current
  if not state then
    return
  end
  if state.context.key.kind == "forge_backed" then
    notice.error("Use :Lgtm submit for GitHub reviews")
    return
  end
  rpc.call(
    "CompleteSession",
    { contextId = state.context.id, requestKey = M.request_key() },
    function(err, result)
      if err then
        notice.error(err)
      else
        state.session = result.session
        notice.show("Local review completed")
      end
    end
  )
end

function M.request_key()
  return vim.fn.sha256(tostring(vim.uv.hrtime()) .. tostring(math.random()))
end

function M.submission_result(state, err, result, pending, done)
  if err then
    notice.error(
      (
        err.uncertain and "Submission response lost. Use :Lgtm reconcile before retrying. "
        or "Submission failed. "
      ) .. (err.message or "Unknown error"),
      pending
    )
    done(err)
    return
  end
  state.session = result.session
  local status = result.session.submissionStatus
  if status == "confirmed" then
    M.submission_drafts[state.session.id] = nil
    notice.show("Review submitted", nil, pending)
    done()
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
      message = (result.session.submissionError or ("Submission " .. tostring(status)))
        .. ". Use :Lgtm reconcile to inspect/recover.",
    }
    notice.error(error, pending)
    done(error)
  end
end

function M.submit()
  local state = contexts.current
  if not state then
    return
  end
  if
    state.session.submissionStatus == "uncertain"
    or state.session.submissionStatus == "pending"
    or state.session.submissionStatus == "posting"
    or state.session.submissionStatus == "sending"
    or state.session.submissionStatus == "reconciling"
    or state.session.submissionStatus == "failed"
  then
    notice.error("Submission is pending or uncertain. Use :Lgtm reconcile before retrying.")
    return
  end
  if state.context.key.kind ~= "forge_backed" then
    notice.error("Local reviews cannot be submitted to GitHub; use :Lgtm complete")
    return
  end
  require("lgtm.picker").select(
    "Submit review",
    { "comment", "approve", "request_changes" },
    tostring,
    function(outcome)
      local draft = M.submission_drafts[state.session.id]
        or { body = state.session.body or "", requestKey = M.request_key() }
      M.submission_drafts[state.session.id] = draft
      require("lgtm.composer").open("Final review body (optional)", draft.body, function(body, done)
        draft.body = body
        draft.outcome = outcome
        local pending = notice.show("Submitting review…")
        rpc.call("SubmitReview", {
          sessionId = state.session.id,
          outcome = outcome,
          body = body,
          requestKey = draft.requestKey,
          expectedHead = state.session.head,
        }, function(err, result)
          M.submission_result(state, err, result, pending, done)
        end)
      end)
    end
  )
end

function M.reconcile()
  local state = contexts.current
  if not state then
    return
  end
  local pending = notice.show("Checking submission status…")
  rpc.call("ReconcileSubmission", { sessionId = state.session.id }, function(err, result)
    M.submission_result(state, err, result, pending, function() end)
  end)
end

function M.reviewed()
  local state = contexts.current
  if not state or not state.file then
    return
  end
  state.reviewed = state.reviewed or {}
  state.reviewed[state.file.path] = state.reviewed[state.file.path] and nil
    or state.file.contentHash
  require("lgtm.files").render(state)
  notice.show(
    state.reviewed[state.file.path] and "File marked reviewed" or "File marked unreviewed"
  )
end
return M
