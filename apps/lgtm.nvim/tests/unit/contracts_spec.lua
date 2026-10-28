describe("When using plugin contracts", function()
  -- fixtures ------------------------------------------------------------------

  local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h:h")
  local config = require("lgtm.config")
  local rpc = require("lgtm.services.client")
  local transport = require("lgtm.transport.connect")
  local store = require("lgtm.services.state")
  local notice = require("lgtm.notifications")
  local saved

  before_each(function()
    saved = {
      config = vim.deepcopy(config.options),
      call = rpc.call,
      identity = rpc.identity,
      capabilities = rpc.capabilities,
      transport = transport.call,
      current = store.current,
      error = notice.error,
    }
    rpc.identity = nil
  end)

  after_each(function()
    config.options = saved.config
    rpc.call, rpc.identity, rpc.capabilities = saved.call, saved.identity, saved.capabilities
    transport.call, notice.error = saved.transport, saved.error
    store.set(saved.current)
  end)

  -- unit tests ----------------------------------------------------------------

  describe("When loading plugin sources", function()
    it("Parses every Lua module and package entry point", function()
      for _, file in ipairs(vim.fn.glob(root .. "/lua/**/*.lua", false, true)) do
        assert(loadfile(file))
      end
      assert(loadfile(root .. "/plugin/lgtm.lua"))
      assert(loadfile(root .. "/../../lazy.lua"))
    end)
  end)

  describe("When normalizing a repository remote", function()
    describe("When the remote names a GitHub repository", function()
      for _, remote in ipairs({
        "https://github.com/acme/repo.git",
        "git@github.com:acme/repo.git",
        "ssh://git@github.com/acme/repo.git",
        "https://github.com/acme/repo/",
      }) do
        it("Normalizes " .. remote, function()
          assert.equals(
            "https://github.com/acme/repo",
            require("lgtm.sessions").repository_url(remote)
          )
        end)
      end
    end)

    describe("When the remote is unsupported", function()
      for _, remote in ipairs({ "git@gitlab.com:acme/repo.git", "/tmp/repo" }) do
        it("Rejects " .. remote, function()
          local actual = require("lgtm.sessions").repository_url(remote)
          assert.is_falsy(actual)
        end)
      end
    end)
  end)

  describe("When configuring a Connect endpoint", function()
    it("Removes a trailing slash", function()
      config.setup({ endpoint_url = "http://127.0.0.1:1/rpc/" })
      assert.equals("http://127.0.0.1:1/rpc", transport.endpoint())
    end)
  end)

  describe("When selecting a thread at a source line", function()
    it("Keeps old-side and new-side ranges separate", function()
      -- Arrange
      local content = "a\nb\nc\nd\ne\n"
      local source = {
        commit = { sha = string.rep("a", 40), contentHash = "sha256:" .. vim.fn.sha256(content) },
      }
      local state = { views = {}, conversation = { comments = {}, threads = {} } }
      for _, side in ipairs({ "old", "new" }) do
        state.views[side] = {
          path = "a",
          logical_path = "a",
          side = side,
          content = content,
          source = require("lgtm.anchors").source_from_wire(source),
        }
        table.insert(state.conversation.comments, { threadId = side })
        table.insert(state.conversation.threads, {
          id = side,
          anchor = {
            kind = "line",
            path = "a",
            side = side,
            start = "2",
            ["end"] = "4",
            source = source,
            evidence = { selectedText = "b\nc\nd", contextBefore = "a", contextAfter = "e" },
          },
        })
      end

      -- Act / Assert
      assert.equals("old", require("lgtm.threads").at(state, "a", "old", 3)[1].id)
      assert.equals(0, #require("lgtm.threads").at(state, "a", "new", 5))
    end)
  end)

  describe("When a backend lacks required capabilities", function()
    it("Rejects identity setup and allows a later supported backend", function()
      -- Arrange
      config.options.identity = { name = "Reviewer", email = "reviewer@example.test" }
      local identity_calls, result = 0, nil
      local capabilities = { "notes" }
      transport.call = function(_, method, _, _, done)
        if method == "GetCapabilities" then
          done(nil, { apiVersion = "1", capabilities = capabilities })
        else
          identity_calls = identity_calls + 1
          done(nil, { actor = { id = "supported-backend-actor" } })
        end
      end

      -- Act
      rpc.bootstrap(function(err)
        result = err
      end)

      -- Assert
      assert.matches("update the LGTM CLI/backend", result.message, 1, true)
      assert.equals(0, identity_calls)
      assert.is_nil(rpc.identity)

      -- Act
      capabilities = rpc.required_capabilities
      rpc.bootstrap(function(err)
        result = err
      end)

      -- Assert
      assert.is_nil(result)
      assert.equals(1, identity_calls)
      assert.equals("supported-backend-actor", rpc.identity)
    end)
  end)

  describe("When a saved review has no recorded comparison", function()
    it("Keeps discussion access without guessing source content", function()
      -- Arrange
      local legacy = {
        id = "legacy",
        version = "0",
        state = "completed",
        target = { localGit = { path = "/missing/legacy", branch = "refs/heads/old" } },
      }
      local message
      notice.error = function(value)
        message = value
      end
      rpc.call = function(method, _, done)
        if method == "GetReview" then
          done(nil, { review = legacy })
        elseif method == "GetConversation" then
          done(nil, { version = "0", comments = {}, threads = {} })
        else
          error("Unavailable source evidence must not invoke content operations: " .. method)
        end
      end
      local state = store.apply({}, legacy)

      -- Act
      require("lgtm.review").open(state)

      -- Assert
      assert.is_truthy(state.conversation)
      assert.is_nil(state.panes)
      assert.matches("no recorded comparison", message, 1, true)
    end)
  end)

  describe("When rejected-submission recovery is unsupported", function()
    for _, operation in ipairs({ "submit", "reconcile" }) do
      it("Blocks " .. operation .. " before invoking an operation that may publish", function()
        -- Arrange
        local message
        notice.error = function(value)
          message = value
        end
        rpc.capabilities = {}
        store.set({ kind = "pr", review = { id = "rejected-upgrade", submissionStatus = "failed" } })
        rpc.call = function()
          error("An older backend recovery method may publish")
        end

        -- Act
        require("lgtm.review")[operation]()

        -- Assert
        assert.matches("Update the LGTM CLI/backend", message, 1, true)
      end)
    end
  end)
end)
