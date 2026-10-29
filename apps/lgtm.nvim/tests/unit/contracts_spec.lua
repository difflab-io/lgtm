describe("When using plugin contracts", function()
  -- fixtures ------------------------------------------------------------------

  local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h:h")
  local config = require("lgtm.config")
  local rpc = require("lgtm.services.client")
  local transport = require("lgtm.services.connect")
  local store = require("lgtm.services.state")
  local notice = require("lgtm.ui.notifications")
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
      system = vim.system,
    }
    rpc.identity = nil
  end)

  after_each(function()
    vim.system = saved.system
    config.options = saved.config
    rpc.call, rpc.identity, rpc.capabilities = saved.call, saved.identity, saved.capabilities
    transport.call, notice.error = saved.transport, saved.error
    store.set(saved.current)
  end)

  local function get_remote_context(remote)
    vim.system = function(arguments, _, callback)
      callback({
        code = 0,
        stdout = vim.list_contains(arguments, "get-url") and remote or "feature\n",
        stderr = "",
      })
      return {}
    end
    local result
    require("lgtm.services.forge_service").get_pr_context(
      "/checkout",
      { remote = "origin" },
      function(context)
        result = context
      end
    )
    assert(
      vim.wait(1000, function()
        return result ~= nil
      end),
      "PR context did not load"
    )
    return result
  end

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
          assert.equal("https://github.com/acme/repo", get_remote_context(remote).repository_url)
        end)
      end
    end)

    describe("When the remote is unsupported", function()
      for _, remote in ipairs({ "git@gitlab.com:acme/repo.git", "/tmp/repo" }) do
        it("Rejects " .. remote, function()
          local actual = get_remote_context(remote).repository_url
          assert.is_falsy(actual)
        end)
      end
    end)
  end)

  describe("When configuring a Connect endpoint", function()
    it("Removes a trailing slash", function()
      config.setup({ endpoint_url = "http://127.0.0.1:1/rpc/" })
      assert.equal("http://127.0.0.1:1/rpc", transport.endpoint())
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
          source = require("lgtm.models.source").source_from_wire(source),
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

      state.placement_views, state.placement_conversation = state.views, state.conversation
      state.placements = {
        old = { old = { status = "exact", location = { kind = "line", start = 2, ["end"] = 4 } } },
        new = { new = { status = "exact", location = { kind = "line", start = 2, ["end"] = 4 } } },
      }

      -- Act / Assert
      assert.equal(
        "old",
        require("lgtm.features.annotation_overlay").at(state, "a", "old", 3)[1].id
      )
      assert.equal(0, #require("lgtm.features.annotation_overlay").at(state, "a", "new", 5))
    end)
  end)

  describe("When a backend lacks required capabilities", function()
    it("Rejects identity setup and allows a later supported backend", function()
      -- Arrange
      config.options.identity = { name = "Reviewer", email = "reviewer@example.test" }
      local identity_calls = 0
      ---@type lgtm.ServiceError?
      local result
      local capabilities = { "notes" }
      rawset(transport, "call", function(_, method, _, _, done)
        if method == "GetCapabilities" then
          done(nil, { apiVersion = "1", capabilities = capabilities })
        else
          identity_calls = identity_calls + 1
          done(nil, { actor = { id = "supported-backend-actor" } })
        end
      end)

      -- Act
      rpc.bootstrap(function(err)
        result = err
      end)

      -- Assert
      assert.matches("update the LGTM CLI/backend", result and result.message or "", 1, true)
      assert.equal(0, identity_calls)
      assert.is_nil(rpc.identity)

      -- Act
      capabilities = rpc.required_capabilities
      rpc.bootstrap(function(err)
        result = err
      end)

      -- Assert
      assert.is_nil(result)
      assert.equal(1, identity_calls)
      assert.equal("supported-backend-actor", rpc.identity)
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
      rawset(notice, "error", function(value)
        message = value
      end)
      rawset(rpc, "call", function(method, _, done)
        if method == "GetReview" then
          done(nil, { review = legacy })
        elseif method == "GetConversation" then
          done(nil, { version = "0", comments = {}, threads = {} })
        else
          error("Unavailable source evidence must not invoke content operations: " .. method)
        end
      end)
      local state = store.apply({}, legacy)

      -- Act
      require("lgtm.app").open(state)

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
        rawset(notice, "error", function(value)
          message = value
        end)
        rpc.capabilities = {}
        store.set({
          kind = "pr",
          review = {
            id = "rejected-upgrade",
            version = "0",
            state = "completed",
            target = {},
            comparison = {},
            submissionStatus = "failed",
          },
        })
        rawset(rpc, "call", function()
          error("An older backend recovery method may publish")
        end)

        -- Act
        require("lgtm.app")[operation]()

        -- Assert
        assert.matches("Update the LGTM CLI/backend", message, 1, true)
      end)
    end
  end)
end)
