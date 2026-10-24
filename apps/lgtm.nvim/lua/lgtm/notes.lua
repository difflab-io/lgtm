local M = { states = {} }
local rpc, notice = require("lgtm.rpc"), require("lgtm.notifications")
local contexts, sources = require("lgtm.context"), require("lgtm.sources")

function M.owns_review_buffer(buf)
  if not vim.api.nvim_buf_is_valid(buf) then
    return false
  end
  if vim.b[buf].lgtm then
    return true
  end
  local lifecycle = package.loaded["codediff.ui.lifecycle"]
  local session = lifecycle and lifecycle.get_session(vim.api.nvim_get_current_tabpage())
  return session and (session.original_bufnr == buf or session.modified_bufnr == buf) or false
end

function M.state(buf, callback)
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" or vim.bo[buf].buftype ~= "" then
    callback({ message = "Open a saved file to use notes" })
    return
  end
  local state = M.states[buf]
  if state and state.pending_location then
    require("lgtm.locations").retry(state, function(err)
      callback(err, not err and state or nil)
    end)
    return
  end
  if state and state.moving then
    callback({ message = "The saved file move is still being recorded" })
    return
  end
  if not state or state.scope.notes.path ~= (vim.uv.fs_realpath(path) or path) then
    state = contexts.notes(path)
    state.buf = buf
    M.states[buf] = state
    state.redraw = function()
      if not M.owns_review_buffer(buf) then
        require("lgtm.threads").attach(state, buf, state.path, "new")
      end
    end
    state.observe_sources = function(done)
      sources.observe(state, { path = state.path }, state.views, done)
    end
    state.render = function(done)
      state.render_callbacks = state.render_callbacks or {}
      if done then
        state.render_callbacks[#state.render_callbacks + 1] = done
      end
      state.render_generation = (state.render_generation or 0) + 1
      local generation = state.render_generation
      sources.file(state.root, state.path, "local_file", function(err, selected)
        if
          M.states[buf] ~= state
          or not vim.api.nvim_buf_is_valid(buf)
          or generation ~= state.render_generation
        then
          return
        end
        state.views = {
          new = vim.tbl_extend("force", selected or {}, {
            path = state.path,
            logical_path = state.path,
            side = "new",
            unavailable = err and err.message,
          }),
        }
        if selected and selected.source then
          local _, mismatch = sources.capture(state.views.new, buf, "file")
          if mismatch then
            state.views.new.unavailable = mismatch.message
          end
        end
        state.redraw()
        local function finish(error)
          if generation ~= state.render_generation then
            return
          end
          local callbacks = state.render_callbacks
          state.render_callbacks = {}
          for _, callback in ipairs(callbacks) do
            callback(error)
          end
        end
        if err then
          finish(err)
          return
        end
        sources.observe(state, { path = state.path }, state.views, finish)
      end)
    end
    state.refresh = function(done)
      contexts.conversation(state, function(err)
        if err then
          if done then
            done(err)
          else
            notice.error(err)
          end
          return
        end
        state.render(done)
      end)
    end
  end
  state.refresh(function(err)
    callback(err, state)
  end)
end

-- Reading or exporting ordinary notes never creates a review or storage context.
function M.get_existing(buf, callback)
  M.state(buf, callback)
end

function M.restore(buf)
  if M.owns_review_buffer(buf) then
    return
  end
  if not vim.api.nvim_buf_is_valid(buf) or vim.bo[buf].buftype ~= "" then
    return
  end
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" then
    return
  end
  local review = contexts.current
  if review and review.temp_dir and path:sub(1, #review.temp_dir) == review.temp_dir then
    return
  end
  M.state(buf, function() end)
end

function M.add(range)
  local buf, first = vim.api.nvim_get_current_buf(), vim.api.nvim_win_get_cursor(0)[1]
  local last = first
  if range then
    first, last = range[1], range[2]
  end
  if vim.fn.mode():match("[vV\22]") then
    first, last = vim.fn.line("v"), vim.fn.line(".")
    first, last = math.min(first, last), math.max(first, last)
  end
  M.state(buf, function(err, state)
    if err then
      notice.error(err)
      return
    end
    local anchor, capture_error = sources.capture(state.views.new, buf, "line", first, last)
    if not anchor then
      notice.error(capture_error)
      return
    end
    -- The backend supplies the relative path, including moved-file associations.
    anchor.path = nil
    local input = {
      scope = state.scope,
      expectedVersion = state.version,
      requestKey = contexts.request_key(),
      anchor = anchor,
    }
    require("lgtm.ui.composer").open("Note", "", function(body, done)
      if input.body and input.body ~= body then
        input.requestKey = contexts.request_key()
      end
      input.body = body
      rpc.call("AddComment", input, function(save_error)
        if save_error then
          done(save_error)
          return
        end
        state.refresh(function(refresh_error)
          if not refresh_error then
            notice.show("Note saved")
          end
          done(refresh_error)
        end)
      end)
    end)
  end)
end

function M.list()
  M.state(vim.api.nvim_get_current_buf(), function(err, state)
    if err then
      notice.error(err)
    else
      require("lgtm.threads").open(state, state.conversation.threads or {}, true)
    end
  end)
end

function M.promote()
  local target = contexts.current
  if
    not target
    or target.initializing
    or target.load_error
    or not target.file
    or not target.panes
  then
    notice.error(
      "Open the target review file first, then open the source note and use :Lgtm promote"
    )
    return
  end
  M.state(vim.api.nvim_get_current_buf(), function(err, source)
    if err then
      notice.error(err)
      return
    end
    local roots = vim.tbl_filter(function(comment)
      return tonumber(comment.position or 0) == 0
    end, source.conversation.comments or {})
    require("lgtm.picker").select("Promote note (original retained)", roots, function(comment)
      return comment.body:gsub("\n", " ")
    end, function(comment)
      local original
      for _, thread in ipairs(source.conversation.threads or {}) do
        if thread.id == comment.threadId then
          original = thread.anchor
        end
      end
      if not original then
        notice.error("Note anchor unavailable")
        return
      end
      local function confirm()
        sources.read(target, target.file, "new", function(read_error, selected)
          if read_error then
            notice.error(read_error)
            return
          end
          local view = vim.tbl_extend("force", selected, { path = target.file.path, side = "new" })
          local anchor, capture_error = sources.capture(
            view,
            target.panes.new.buf,
            original.kind,
            tonumber(original.start),
            tonumber(original["end"])
          )
          if not anchor then
            notice.error(capture_error)
            return
          end
          require("lgtm.picker").select(
            "Promote to " .. target.file.path .. ":" .. tostring(anchor.start or "file"),
            { "Cancel", "Promote" },
            tostring,
            function(choice)
              if choice ~= "Promote" then
                return
              end
              rpc.call("PromoteNotes", {
                source = source.scope,
                targetReviewId = target.review.id,
                expectedSourceVersion = source.version,
                expectedReviewVersion = target.version,
                commentIds = { comment.id },
                anchors = { anchor },
                requestKey = contexts.request_key(),
              }, function(error, result)
                if error then
                  notice.error(error)
                  return
                end
                target.refresh()
                if #(result.syncErrors or {}) > 0 then
                  notice.error("Note promoted locally; GitHub sync needs recovery. Use :Lgtm sync.")
                else
                  notice.show("Note promoted; original retained")
                end
              end)
            end
          )
        end)
      end
      require("lgtm.picker").input("Target relative file path: ", target.file.path, function(path)
        target.show_file(path, nil, confirm)
      end)
    end)
  end)
end
return M
