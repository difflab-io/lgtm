local M = { states = {} }
local rpc, notice = require("lgtm.rpc"), require("lgtm.notifications")
-- Read an existing notes session; exports must not create contexts or successors.
function M.get_existing(buf, callback)
  local path = vim.api.nvim_buf_get_name(buf)
  local root = path ~= "" and vim.uv.fs_realpath(vim.fn.fnamemodify(path, ":h"))
  if not root then
    callback({ message = "Open a saved file to export its notes" })
    return
  end
  rpc.call("ListSessions", { path = root, completed = true }, function(err, result)
    if err then
      callback(err)
      return
    end
    local selected
    for _, entry in ipairs(result.entries or {}) do
      if
        entry.context.key.kind == "notes"
        and entry.context.key.path == root
        and (
          not selected
          or entry.session.state == "active"
          or (
            selected.session.state ~= "active"
            and (entry.session.createdAt or "") > (selected.session.createdAt or "")
          )
        )
      then
        selected = entry
      end
    end
    if selected then
      callback(nil, selected)
    else
      callback({ message = "No saved notes for this location" })
    end
  end)
end

function M.state(buf, callback)
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" or vim.bo[buf].buftype ~= "" then
    callback({ message = "Open a named file to add notes" })
    return
  end
  local root, relative = vim.fn.fnamemodify(path, ":h"), vim.fn.fnamemodify(path, ":t")
  require("lgtm.context").notes(root, function(err, state)
    if err then
      callback(err)
      return
    end
    state.note = true
    state.path = relative
    state.refresh = function(done)
      rpc.call(
        "GetConversation",
        { contextId = state.context.id, includeDrafts = true },
        function(error, result)
          if not error then
            state.conversation = result
            require("lgtm.threads").render(state, buf, relative, "new")
          end
          if done then
            done(error)
          end
        end
      )
    end
    M.states[buf] = state
    state.refresh(function(error)
      callback(error, state)
    end)
  end)
end
-- Restore persisted annotations without creating contexts or sessions merely by browsing.
function M.restore(buf)
  if M.states[buf] then
    return
  end
  if not vim.api.nvim_buf_is_valid(buf) or vim.bo[buf].buftype ~= "" then
    return
  end
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" then
    return
  end
  local review = require("lgtm.context").current
  if review and review.temp_dir and path:sub(1, #review.temp_dir) == review.temp_dir then
    return
  end
  local root = vim.uv.fs_realpath(vim.fn.fnamemodify(path, ":h"))
  if not root then
    return
  end
  local relative = vim.fn.fnamemodify(path, ":t")
  rpc.call("ListSessions", { path = root, completed = true }, function(err, response)
    if
      err
      or M.states[buf]
      or not vim.api.nvim_buf_is_valid(buf)
      or vim.api.nvim_buf_get_name(buf) ~= path
    then
      return
    end
    local entry
    for _, item in ipairs(response.entries or {}) do
      if item.context.key.kind == "notes" and item.context.key.path == root then
        entry = item
        break
      end
    end
    if not entry then
      return
    end
    local state = {
      context = entry.context,
      session = entry.session,
      root = root,
      path = relative,
      note = true,
    }
    state.refresh = function(done)
      rpc.call(
        "GetConversation",
        { contextId = state.context.id, includeDrafts = true },
        function(error, result)
          if not error and vim.api.nvim_buf_is_valid(buf) then
            state.conversation = result
            require("lgtm.threads").attach(state, buf, relative, "new")
          end
          if done then
            done(error)
          end
        end
      )
    end
    M.states[buf] = state
    state.refresh()
  end)
end

function M.add(range)
  local buf, line = vim.api.nvim_get_current_buf(), vim.api.nvim_win_get_cursor(0)[1]
  local last = line
  if range then
    line, last = range[1], range[2]
  end
  if vim.fn.mode():match("[vV\22]") then
    line, last = vim.fn.line("v"), vim.fn.line(".")
    line, last = math.min(line, last), math.max(line, last)
  end
  M.state(buf, function(err, state)
    if err then
      notice.error(err)
      return
    end
    rpc.call(
      "GetWorkspace",
      { contextId = state.context.id, includeIgnored = true },
      function(error, workspace)
        if error then
          notice.error(error)
          return
        end
        local hash
        for _, file in ipairs(workspace.files or {}) do
          if file.path == state.path then
            hash = file.contentHash
          end
        end
        if not hash then
          notice.error("File is unavailable to the backend. Save it before adding a note.")
          return
        end
        require("lgtm.composer").open("Note", "", function(body, done)
          rpc.call("AddComment", {
            sessionId = state.session.id,
            body = body,
            anchor = {
              kind = "line",
              path = state.path,
              start = line,
              ["end"] = last,
              side = "new",
              contentHash = hash,
            },
          }, function(save_err)
            if save_err then
              done(save_err)
              return
            end
            state.refresh(function(refresh_err)
              if not refresh_err then
                require("lgtm.threads").attach(state, buf, state.path, "new")
                notice.show("Note saved")
              end
              done(refresh_err)
            end)
          end)
        end)
      end
    )
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
  local target = require("lgtm.context").current
  if not target then
    notice.error("Select a target review first, then open the source file and use :Lgtm promote")
    return
  end
  M.state(vim.api.nvim_get_current_buf(), function(err, source)
    if err then
      notice.error(err)
      return
    end
    require("lgtm.picker").select(
      "Promote note (original retained)",
      source.conversation.comments or {},
      function(comment)
        return comment.body:gsub("\n", " ")
      end,
      function(comment)
        local anchor
        for _, thread in ipairs(source.conversation.threads or {}) do
          if thread.id == comment.threadId then
            anchor = vim.deepcopy(thread.anchor)
          end
        end
        if not anchor then
          notice.error("Note anchor unavailable")
          return
        end
        require("lgtm.picker").input("Target relative file path: ", anchor.path, function(path)
          anchor.path = path
          if target.context.key.kind ~= "notes" then
            anchor.contentHash = nil
            anchor.commit = anchor.side == "old" and target.session.base or target.session.head
          else
            for _, file in ipairs(target.files or {}) do
              if file.path == path then
                anchor.contentHash = file.contentHash
                anchor.commit = nil
              end
            end
          end
          require("lgtm.picker").select(
            "Target anchor: " .. path .. ":" .. tostring(anchor.start or "file"),
            { "Cancel", "Promote" },
            tostring,
            function(choice)
              if choice ~= "Promote" then
                return
              end
              rpc.call("PromoteNotes", {
                sourceContextId = source.context.id,
                targetSessionId = target.session.id,
                commentIds = { comment.id },
                anchors = { anchor },
                requestKey = require("lgtm.review").request_key(),
              }, function(error)
                if error then
                  notice.error(error)
                else
                  target.refresh()
                  notice.show("Note promoted; original retained")
                end
              end)
            end
          )
        end)
      end
    )
  end)
end
return M
