describe("When capturing local source content", function()
  -- fixtures ------------------------------------------------------------------

  local sources = require("lgtm.services.sources")
  local directory
  local git

  before_each(function()
    directory = vim.fn.tempname()
    vim.fn.mkdir(directory, "p")
    git("init", "-q")
    git("config", "user.name", "LGTM test")
    git("config", "user.email", "lgtm@example.test")
  end)

  after_each(function()
    vim.fn.delete(directory, "rf")
  end)

  local function command(args)
    local result = vim.system(args, { text = true }):wait()
    assert(result.code == 0, result.stderr)
    return vim.trim(result.stdout)
  end

  git = function(...)
    local args = { "git", "-C", directory }
    vim.list_extend(args, { ... })
    return command(args)
  end

  local function read(method, ...)
    local arguments, finished, error, result = { ... }, false, nil, nil
    arguments[#arguments + 1] = function(err, value)
      error, result, finished = err, value, true
    end
    method(unpack(arguments))
    assert(
      vim.wait(10000, function()
        return finished
      end),
      "Source read timed out"
    )
    return error, result
  end

  local function write(path, bytes)
    local fd = assert(vim.uv.fs_open(directory .. "/" .. path, "w", 438))
    assert(vim.uv.fs_write(fd, bytes, 0))
    assert(vim.uv.fs_close(fd))
  end

  -- integration tests ---------------------------------------------------------

  describe("When reading real Git and filesystem sources", function()
    describe("When index and working bytes differ", function()
      it("Captures exact staged bytes without changing the index or HEAD", function()
        local path = "space [literal] ü.txt"
        write(path, "before\r\nselected\r\nafter\r\n")
        git("add", "--", path)
        git("commit", "-qm", "fixture")
        local commit = git("rev-parse", "HEAD")
        local _, committed = read(sources.commit, directory, path, commit)
        assert(committed.content == "before\r\nselected\r\nafter\r\n")
        assert(committed.source.sha == commit)
        write(path, "before\r\nstaged\r\nafter")
        git("add", "--", path)
        local _, staged = read(sources.index, directory, path)
        assert(staged.source.kind == "index_file")
        assert(staged.content == "before\r\nstaged\r\nafter")
        assert(staged.source.blob_oid ~= "")
        local _, working = read(sources.working, directory, path)
        assert(
          vim.deep_equal(working.source, staged.source),
          "Saved working bytes equal index and are preservable"
        )
        write(path, "before\r\nunstaged\r\nafter\r\n")
        _, working = read(sources.working, directory, path)
        assert(working.source.kind == "working_file")
        assert(working.source.content_hash ~= staged.source.content_hash)
        local _, still_staged = read(sources.index, directory, path)
        assert(vim.deep_equal(staged, still_staged), "Reading working text must not stage it")
        local buf = vim.api.nvim_create_buf(false, true)
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, { "before", "staged", "after" })
        local view = vim.tbl_extend("force", staged, { path = path, side = "new" })
        local anchor = assert(sources.capture(view, buf, "line", 2, 2))
        assert(anchor.source.indexFile.blobOid == staged.source.blob_oid)
        assert(anchor.evidence.selectedText == "staged")
        assert(
          anchor.evidence.contextBefore == "before" and anchor.evidence.contextAfter == "after"
        )
        assert(not anchor.commit and not anchor.contentHash, "No fabricated HEAD identity")
        vim.api.nvim_buf_set_lines(buf, 1, 2, false, { "unsaved" })
        assert(
          not sources.capture(view, buf, "line", 2, 2),
          "Unsaved buffer mismatch must be rejected"
        )
        assert(git("rev-parse", "HEAD") == commit, "Source reads do not commit")
        vim.api.nvim_buf_delete(buf, { force = true })
      end)
    end)

    describe("When files are missing or unsafe to review", function()
      it("Distinguishes missing, empty, binary and escaping sources", function()
        local _, missing = read(sources.file, directory, "missing", "local_file")
        assert(not missing.source and missing.content == nil)
        write("empty", "")
        local _, empty = read(sources.file, directory, "empty", "local_file")
        assert(empty.content == "" and empty.source.content_hash == "sha256:" .. vim.fn.sha256(""))
        local buf = vim.api.nvim_create_buf(false, true)
        local view = vim.tbl_extend("force", empty, { path = "empty", side = "new" })
        assert(sources.capture(view, buf, "file"))
        assert(not sources.capture(view, buf, "line", 1, 1), "Empty file has no real line 1")
        vim.api.nvim_buf_delete(buf, { force = true })
        write("binary", "a\0b")
        assert(read(sources.file, directory, "binary", "local_file"))
        write("invalid-utf8", "\237\160\128")
        assert(read(sources.file, directory, "invalid-utf8", "local_file"))
        local outside = vim.fn.tempname()
        vim.fn.writefile({ "outside" }, outside)
        assert(vim.uv.fs_symlink(outside, directory .. "/escape"))
        assert(read(sources.file, directory, "escape", "local_file"))
        vim.fn.delete(outside)
        git("add", "--", "escape")
        assert(
          read(sources.index, directory, "escape"),
          "Git symlink content is not reviewable source"
        )
      end)
    end)
  end)
end)
