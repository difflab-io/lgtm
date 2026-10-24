-- Read only the selected source. All identities describe exact bytes, never joined buffer lines.
-- State -----------------------------------------------------------------------

local M = { max_bytes = 2 * 1024 * 1024 }
local Private = {}
local uv = vim.uv
local anchors = require("lgtm.anchors")

-- Metadata invalidates a viewed mark without reading a hidden file's body.
-- Public ----------------------------------------------------------------------

function M.saved_stamp(stat)
  if not stat or stat.type ~= "file" then
    return false
  end
  return { size = stat.size, mtime = stat.mtime, ctime = stat.ctime, ino = stat.ino, dev = stat.dev }
end

-- fs_realpath excludes symlinks escaping a directory review; Git symlinks are rejected by mode.
function M.file(root, path, kind, callback)
  local absolute = vim.fs.joinpath(root, path)
  uv.fs_realpath(root, function(root_error, real_root)
    uv.fs_realpath(absolute, function(path_error, real_path)
      if root_error or path_error then
        vim.schedule(function()
          if path_error and path_error:match("ENOENT") then
            callback(nil, {})
          else
            callback(Private.failure("Saved source is unavailable: " .. absolute))
          end
        end)
        return
      end
      if real_path ~= real_root and real_path:sub(1, #real_root + 1) ~= real_root .. "/" then
        vim.schedule(function()
          callback(Private.failure("Source is outside the selected directory"))
        end)
        return
      end
      if real_path ~= vim.fs.normalize(vim.fs.joinpath(real_root, path)) then
        vim.schedule(function()
          callback(
            Private.failure("Review source cannot traverse a symlink inside the selected directory")
          )
        end)
        return
      end
      uv.fs_open(real_path, "r", 438, function(open_error, fd)
        if open_error then
          vim.schedule(function()
            callback(Private.failure("Cannot read saved source: " .. absolute))
          end)
          return
        end
        uv.fs_fstat(fd, function(stat_error, stat)
          if stat_error or stat.type ~= "file" or stat.size > M.max_bytes then
            uv.fs_close(fd)
            vim.schedule(function()
              callback(Private.failure("Review source must be a regular file of at most 2 MiB"))
            end)
            return
          end
          uv.fs_read(fd, M.max_bytes + 1, 0, function(read_error, bytes)
            uv.fs_close(fd)
            vim.schedule(function()
              if read_error then
                callback(Private.failure("Cannot read saved source: " .. absolute))
              else
                Private.finish(kind or "local_file", bytes, nil, function(err, selected)
                  if selected then
                    selected.saved_stamp = M.saved_stamp(stat)
                  end
                  callback(err, selected)
                end)
              end
            end)
          end)
        end)
      end)
    end)
  end)
end

function M.index(root, path, callback)
  Private.git(root, { "ls-files", "--stage", "-z", "--", path }, function(err, output)
    if err then
      callback(err)
      return
    end
    if output == "" then
      callback(nil, {})
      return
    end
    local mode, oid, stage, entry = output:match("^(%d+) (%x+) (%d)\t(.-)%z$")
    if entry ~= path or stage ~= "0" or (mode ~= "100644" and mode ~= "100755") then
      callback(
        Private.failure("Review source requires a regular, unconflicted stage-zero index entry")
      )
      return
    end
    Private.blob(root, oid, "index_file", { blob_oid = oid }, callback)
  end)
end

function M.commit(root, path, commit, callback)
  if type(commit) ~= "string" or not commit:match("^%x+$") or (#commit ~= 40 and #commit ~= 64) then
    callback(Private.failure("Review source requires a full commit ID"))
    return
  end
  Private.git(root, { "ls-tree", "-z", commit, "--", path }, function(err, output)
    if err then
      callback(err)
      return
    end
    if output == "" then
      callback(nil, {})
      return
    end
    local mode, oid, entry = output:match("^(%d+) blob (%x+)\t(.-)%z$")
    if entry ~= path or (mode ~= "100644" and mode ~= "100755") then
      callback(Private.failure("Review source requires a regular Git blob"))
      return
    end
    Private.blob(root, oid, "commit", { sha = commit }, callback)
  end)
end

-- Working bytes are preservable index bytes only when the entire saved file matches.
function M.working(root, path, callback)
  M.file(root, path, "working_file", function(err, working)
    if err or not working.source then
      callback(err, working)
      return
    end
    M.index(root, path, function(_, indexed)
      if indexed and indexed.source and indexed.content == working.content then
        indexed.saved_stamp = working.saved_stamp
        callback(nil, indexed)
      else
        callback(nil, working)
      end
    end)
  end)
end

---@param state lgtm.ReviewState
function M.read(state, file, side, callback)
  local path = side == "old" and (file.previousPath or file.old_path or file.path) or file.path
  if state.git_root then
    if side == "old" then
      M.commit(state.git_root, path, state.base, callback)
    elseif state.mode == "working" then
      M.working(state.git_root, path, callback)
    elseif state.mode == "staged" then
      M.index(state.git_root, path, callback)
    else
      M.commit(state.git_root, path, state.head, callback)
    end
  elseif file.sides then
    local selected = file.sides[side]
    callback(nil, selected and {
      content = selected.content or "",
      source = anchors.source_from_wire(selected.source),
    } or {})
  else
    M.file(state.root, path, "local_file", callback)
  end
end

function M.wire(source)
  local value = { contentHash = source.content_hash }
  if source.kind == "commit" then
    value.sha = source.sha
  elseif source.kind == "index_file" then
    value.blobOid = source.blob_oid
  end
  local field = ({
    commit = "commit",
    index_file = "indexFile",
    working_file = "workingFile",
    local_file = "localFile",
  })[source.kind]
  return { [field] = value }
end

-- A composer keeps this capture and its operation key until a confirmed save.
function M.capture(view, buf, kind, first, last)
  if kind == "general" then
    return { kind = "general" }
  end
  if not view or not view.source or view.content == nil then
    return nil, Private.failure("Selected source is unavailable")
  end
  if vim.bo[buf].buftype == "" and vim.bo[buf].modified then
    return nil,
      Private.failure("Unsaved buffer. Save to restore comment locations and add comments.")
  end
  local lines = anchors.lines(view.content)
  local displayed = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  -- CodeDiff may retain CR bytes in Git buffers instead of applying 'fileformat'.
  for index, line in ipairs(displayed) do
    if line:sub(-1) == "\r" and lines[index] == line:sub(1, -2) then
      displayed[index] = line:sub(1, -2)
    end
  end
  local expected = #lines == 0 and { "" } or lines
  if not vim.deep_equal(expected, displayed) then
    return nil,
      Private.failure(
        "Displayed text differs from the saved source. Save and refresh before commenting."
      )
  end
  local anchor =
    { kind = kind, path = view.path, source = M.wire(view.source), evidence = vim.empty_dict() }
  if kind == "line" then
    if not first or not last or first < 1 or last < first or last > #lines then
      return nil,
        Private.failure("Select existing source lines; use a file comment for an empty file")
    end
    anchor.side, anchor.start, anchor["end"] = view.side, first, last
    anchor.evidence = {
      selectedText = table.concat(lines, "\n", first, last),
      contextBefore = table.concat(lines, "\n", math.max(1, first - 3), first - 1),
      contextAfter = table.concat(lines, "\n", last + 1, math.min(#lines, last + 3)),
    }
    if
      #anchor.evidence.selectedText > 65536
      or #anchor.evidence.contextBefore > 8192
      or #anchor.evidence.contextAfter > 8192
    then
      return nil, Private.failure("Selected source excerpt is too large")
    end
  end
  return anchor
end

-- One observation batch after a selected source read. No timer or provider synchronization runs here.
---@param state lgtm.ReviewState
function M.observe(state, file, views, callback)
  callback = callback or function() end
  if state.observing then
    state.observe_pending = true
    callback()
    return
  end
  if
    state.kind == "pr"
    or state.initializing
    or state.load_error
    or state.moving
    or not state.conversation
  then
    callback()
    return
  end
  local selected = views.new
  if not selected or selected.unavailable then
    callback()
    return
  end
  local changes, hidden, absence = {}, {}, {}
  local root = state.git_root or state.root
  local function absent(path)
    if absence[path] == nil then
      local stat, _, code =
        uv.fs_lstat(path:sub(1, 1) == "/" and path or vim.fs.joinpath(root, path))
      absence[path] = stat == nil and code == "ENOENT"
    end
    return absence[path]
  end
  local has_comments, renames = {}, {}
  for _, comment in ipairs(state.conversation.comments or {}) do
    has_comments[comment.threadId] = true
  end
  for _, candidate in ipairs(state.files or {}) do
    if candidate.confirmedRename then
      renames[candidate.previousPath] = true
    end
  end
  for _, thread in ipairs(state.conversation.threads or {}) do
    local original, evidence = anchors.from_wire(thread.anchor or {})
    local path = thread.currentPath and thread.currentPath ~= "" and thread.currentPath
      or original.path
    local matches = state.note
      or path == file.path
      or state.kind == "directory" and path == vim.fs.joinpath(root, file.path)
    if has_comments[thread.id] and original.kind ~= "general" then
      if
        file.confirmedRename
        and file.previousPath == path
        and selected.source
        and absent(path)
      then
        changes[#changes + 1] = {
          moved = {
            threadId = thread.id,
            previousPath = path,
            currentPath = file.path,
            source = M.wire(selected.source),
          },
        }
      elseif
        matches
        and not selected.source
        and thread.orphanReason ~= "file_deleted"
        and absent(state.note and state.path or path)
      then
        changes[#changes + 1] = { deleted = { threadId = thread.id, path = path } }
      elseif
        not matches
        and not renames[path]
        and thread.orphanReason ~= "file_deleted"
        and absent(path)
      then
        hidden[path] = hidden[path] or {}
        hidden[path][#hidden[path] + 1] = thread.id
      elseif matches and selected.source and evidence then
        if
          evidence.source.kind == "working_file"
          and evidence.source.content_hash ~= selected.source.content_hash
          and thread.orphanReason ~= "working_source_changed"
        then
          changes[#changes + 1] = {
            workingSourceChanged = {
              threadId = thread.id,
              path = path,
              source = M.wire(selected.source),
            },
          }
        elseif thread.orphanReason and thread.orphanReason ~= "" then
          local matching = evidence.source.content_hash == selected.source.content_hash
            or original.kind == "line"
              and evidence.source.kind ~= "working_file"
              and anchors.matching_range(evidence, selected.content)
          if matching then
            changes[#changes + 1] =
              { restored = { threadId = thread.id, path = path, source = M.wire(selected.source) } }
          end
        end
      end
    end
  end
  local function send(present)
    for path, ids in pairs(hidden) do
      if not present[path] then
        for _, id in ipairs(ids) do
          changes[#changes + 1] = { deleted = { threadId = id, path = path } }
        end
      end
    end
    if #changes == 0 then
      state.observing = false
      callback()
      return
    end
    if #changes > 500 then
      state.observing = false
      callback(Private.failure("Selected comparison exceeds 500 source observations"))
      return
    end
    state.observing = true
    local input = {
      scope = state.scope,
      expectedVersion = state.version,
      sourceChanges = changes,
      requestKey = require("lgtm.services.state").request_key(),
    }
    require("lgtm.services.client").call("RefreshReview", input, function(err, result)
      if not err then
        if result.review then
          require("lgtm.services.state").apply(state, result.review)
        else
          state.version = result.version
        end
      end
      -- Read accepted state before another observation can use its version. Never retry a write blindly.
      require("lgtm.services.reviews").get_conversation(state, function(read_error)
        state.observing = false
        if not read_error and (state.note or require("lgtm.services.state").current == state) then
          if state.redraw then
            state.redraw()
          elseif state.render then
            state.render()
          end
        end
        callback(err or read_error)
        local pending = state.observe_pending
        state.observe_pending = nil
        if pending and not err and not read_error and state.observe_sources then
          vim.schedule(state.observe_sources)
        end
      end)
    end)
  end
  local paths = vim.tbl_keys(hidden)
  if #paths > 500 then
    callback(Private.failure("Comparison exceeds 500 saved source paths"))
    return
  end
  if #paths > 0 and state.kind == "git" and (state.mode == "branch" or state.mode == "staged") then
    -- Absence from the visible diff is not deletion. Check only saved paths in the selected tree/index.
    state.observing = true
    local arguments = state.mode == "staged" and { "ls-files", "--stage", "-z", "--" }
      or { "ls-tree", "-r", "-z", state.head, "--" }
    vim.list_extend(arguments, paths)
    Private.git(root, arguments, function(err, output)
      if err then
        state.observing = false
        callback(err)
        return
      end
      local present = {}
      for entry in output:gmatch("([^%z]+)%z") do
        local path = entry:match("\t(.*)$")
        if path then
          present[path] = true
        end
      end
      send(present)
    end)
  else
    send({})
  end
end

-- Private ---------------------------------------------------------------------

function Private.failure(message)
  return { message = message }
end

function Private.valid_text(bytes)
  if #bytes > M.max_bytes or bytes:find("\0", 1, true) then
    return false
  end
  local i = 1
  while i <= #bytes do
    local a, b, c, d = bytes:byte(i, i + 3)
    local function continuation(value)
      return value and value >= 0x80 and value <= 0xBF
    end
    local count
    if a < 0x80 then
      count = 1
    elseif a >= 0xC2 and a <= 0xDF and continuation(b) then
      count = 2
    elseif
      a >= 0xE0
      and a <= 0xEF
      and continuation(b)
      and continuation(c)
      and (a ~= 0xE0 or b >= 0xA0)
      and (a ~= 0xED or b < 0xA0)
    then
      count = 3
    elseif
      a >= 0xF0
      and a <= 0xF4
      and continuation(b)
      and continuation(c)
      and continuation(d)
      and (a ~= 0xF0 or b >= 0x90)
      and (a ~= 0xF4 or b < 0x90)
    then
      count = 4
    else
      return false
    end
    i = i + count
  end
  return true
end

function Private.finish(kind, bytes, fields, callback)
  if not Private.valid_text(bytes) then
    callback(
      Private.failure("Review source must be UTF-8 text without NUL bytes and at most 2 MiB")
    )
    return
  end
  callback(nil, {
    content = bytes,
    source = vim.tbl_extend("force", fields or {}, {
      kind = kind,
      content_hash = "sha256:" .. vim.fn.sha256(bytes),
    }),
  })
end

function Private.git(root, arguments, callback)
  local command = { "git", "--literal-pathspecs", "-C", root }
  vim.list_extend(command, arguments)
  vim.system(command, { text = false, timeout = 5000 }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        callback(Private.failure(vim.trim(result.stderr or "Git source is unavailable")))
      else
        callback(nil, result.stdout or "")
      end
    end)
  end)
end

function Private.blob(root, oid, kind, fields, callback)
  Private.git(root, { "cat-file", "-s", oid }, function(err, size)
    if err then
      callback(err)
    elseif not tonumber(size) or tonumber(size) > M.max_bytes then
      callback(Private.failure("Review source exceeds 2 MiB"))
    else
      Private.git(root, { "cat-file", "blob", oid }, function(read_error, bytes)
        if read_error then
          callback(read_error)
        else
          Private.finish(kind, bytes, fields, callback)
        end
      end)
    end
  end)
end

return M
