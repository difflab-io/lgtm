-- Record observed filesystem moves. No searching, polling, or Git index mutation.
local M = {}
local contexts, rpc = require("lgtm.context"), require("lgtm.rpc")

function M.canonical(path)
  local absolute = vim.fn.fnamemodify(path, ":p"):gsub("/$", "")
  local ancestor = absolute
  while ancestor do
    local real = vim.uv.fs_realpath(ancestor)
    if real then
      return real .. absolute:sub(#ancestor + 1)
    end
    ancestor = vim.fs.dirname(ancestor)
    if ancestor == "/" then
      return absolute
    end
  end
  return absolute
end

local function relocated(path, from, to)
  if path == from then
    return to
  elseif path:sub(1, #from + 1) == from .. "/" then
    return to .. path:sub(#from + 1)
  end
end

local function accept(state, pending, result, callback)
  state.pending_location, state.moving = nil, true
  state.version = result.version or state.version
  state.views = {}
  if state.note then
    local target = contexts.notes(pending.destination)
    state.scope, state.scope_key, state.root, state.path =
      target.scope, target.scope_key, target.root, target.path
    if state.buf and not vim.api.nvim_buf_is_valid(state.buf) then
      state.moving = false
      callback()
      return
    end
    state.refresh(function(err)
      state.moving = false
      if not err and state.observe_sources then
        state.observe_sources(callback)
      else
        callback(err)
      end
    end)
  else
    contexts.apply(state, result.review)
    state.render_ready = false
    state.source_generation = (state.source_generation or 0) + 1
    if state.render then
      state.render()
    end
    contexts.conversation(state, function(err)
      state.moving = false
      state.reload_pending = true
      if not err and not state.closed and state.tab == vim.api.nvim_get_current_tabpage() then
        M.reload(state, function(error)
          if error then
            require("lgtm.notifications").error(error)
          end
        end)
      end
      callback(err)
    end)
  end
end

function M.reload(state, callback)
  local path = state.file and state.file.path
  state.reload_pending = nil
  require("lgtm.review").load_diff(state, function(err)
    if err then
      callback(err)
    elseif not state.closed and state.tab == vim.api.nvim_get_current_tabpage() then
      local selected
      for _, file in ipairs(state.files) do
        if file.path == path then
          selected = file
        end
      end
      selected = selected or state.files[1]
      if selected then
        state.show_file(selected.path, nil, callback)
      else
        callback()
      end
    else
      callback()
    end
  end)
end

function M.retry(state, callback, can_rebase)
  local pending = state.pending_location
  if not pending or state.moving then
    callback({ message = "A saved location move is already being recorded" })
    return
  end
  state.moving = true
  rpc.call("RefreshReview", pending.input, function(err, result)
    if err then
      if
        can_rebase ~= false
        and err.code == "failed_precondition"
        and (err.message or ""):find("Review data changed", 1, true)
      then
        contexts.conversation(state, function(read_error)
          state.moving = false
          if read_error then
            callback(read_error)
            return
          end
          -- The rejected request has no receipt. Read the intervening write before creating a new guarded operation.
          pending.input.expectedVersion, pending.input.requestKey =
            state.version, contexts.request_key()
          M.retry(state, callback, false)
        end)
        return
      end
      state.moving = false
      callback(err)
    else
      accept(state, pending, result, callback)
    end
  end)
end

local function record(state, from, destination, callback)
  if state.pending_location then
    M.retry(state, callback)
    return
  end
  state.moving = true
  contexts.conversation(state, function(err)
    if err then
      state.moving = false
      callback(err)
      return
    end
    local pending = {
      destination = destination,
      input = {
        scope = vim.deepcopy(state.scope),
        expectedVersion = state.version,
        requestKey = contexts.request_key(),
        sourceChanges = { { locationMoved = { previousPath = from, currentPath = destination } } },
      },
    }
    if state.note and #(state.conversation.threads or {}) == 0 then
      accept(state, pending, { version = state.version }, callback)
      return
    end
    state.pending_location, state.moving = pending, false
    M.retry(state, callback)
  end)
end

function M.rename(from, to)
  from, to = M.canonical(from), M.canonical(to)
  if from == to then
    return
  end
  local source, _, code = vim.uv.fs_lstat(from)
  if source or code ~= "ENOENT" or not vim.uv.fs_stat(to) then
    return
  end
  local changes = {}
  for _, state in pairs(contexts.tabs) do
    if state.kind == "directory" and not state.closed and not state.moving then
      local destination = relocated(state.root, from, to)
      if destination then
        changes[#changes + 1] = { state, state.root, destination }
      end
    end
  end
  for buf, state in pairs(require("lgtm.notes").states) do
    local old = state.scope.notes.path
    local destination = relocated(old, from, to)
    if destination and not state.moving and vim.api.nvim_buf_is_valid(buf) then
      changes[#changes + 1] = { state, old, destination }
    end
  end
  local function next_change(index)
    local change = changes[index]
    if not change then
      return
    end
    record(change[1], change[2], change[3], function(err)
      if err then
        require("lgtm.notifications").error(err)
      end
      next_change(index + 1)
    end)
  end
  next_change(1)
end

return M
