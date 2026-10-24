local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
vim.opt.runtimepath:prepend(root)
-- tests -----------------------------------------------------------------------
local function should_validate_plugin_contracts()
  for _, file in ipairs(vim.fn.glob(root .. "/lua/**/*.lua", false, true)) do
    assert(loadfile(file))
  end
  assert(loadfile(root .. "/plugin/lgtm.lua"))
  assert(loadfile(root .. "/../../lazy.lua"))
  local repository_url = require("lgtm.sessions").repository_url
  for _, remote in ipairs({
    "https://github.com/acme/repo.git",
    "git@github.com:acme/repo.git",
    "ssh://git@github.com/acme/repo.git",
    "https://github.com/acme/repo/",
  }) do
    assert(repository_url(remote) == "https://github.com/acme/repo")
  end
  assert(not repository_url("git@gitlab.com:acme/repo.git"))
  assert(not repository_url("/tmp/repo"))
  local transport = require("lgtm.transport.connect")
  require("lgtm.config").setup({ endpoint_url = "http://127.0.0.1:1/rpc/" })
  assert(transport.endpoint() == "http://127.0.0.1:1/rpc")
  local content = "a\nb\nc\nd\ne\n"
  local source =
    { commit = { sha = string.rep("a", 40), contentHash = "sha256:" .. vim.fn.sha256(content) } }
  local evidence = { selectedText = "b\nc\nd", contextBefore = "a", contextAfter = "e" }
  local states = {
    views = {
      old = {
        path = "a",
        logical_path = "a",
        side = "old",
        content = content,
        source = require("lgtm.anchors").source_from_wire(source),
      },
      new = {
        path = "a",
        logical_path = "a",
        side = "new",
        content = content,
        source = require("lgtm.anchors").source_from_wire(source),
      },
    },
    conversation = {
      comments = { { threadId = "old" }, { threadId = "new" } },
      threads = {
        {
          id = "old",
          anchor = {
            kind = "line",
            path = "a",
            side = "old",
            start = "2",
            ["end"] = "4",
            source = source,
            evidence = evidence,
          },
        },
        {
          id = "new",
          anchor = {
            kind = "line",
            path = "a",
            side = "new",
            start = "2",
            ["end"] = "4",
            source = source,
            evidence = evidence,
          },
        },
      },
    },
  }
  assert(require("lgtm.threads").at(states, "a", "old", 3)[1].id == "old")
  assert(#require("lgtm.threads").at(states, "a", "new", 5) == 0)
  local health_errors = {}
  local original_health = vim.health
  vim.health = {
    start = function() end,
    ok = function() end,
    info = function() end,
    error = function(message)
      health_errors[#health_errors + 1] = message
    end,
  }
  require("lgtm.health").check()
  vim.health = original_health
  assert(table.concat(health_errors, "\n"):find("Missing dependency which-key", 1, true))
  assert(table.concat(health_errors, "\n"):find("Backend unavailable", 1, true))
  assert(table.concat(health_errors, "\n"):find("Missing dependency nui-components", 1, true))
  assert(table.concat(health_errors, "\n"):find("Missing dependency nui.split", 1, true))
  local rpc = require("lgtm.rpc")
  local old_call, old_identity, old_caps = transport.call, rpc.identity, rpc.capabilities
  local old_config = require("lgtm.config").options.identity
  require("lgtm.config").options.identity = { name = "Reviewer", email = "reviewer@example.test" }
  local identity_calls, result = 0, nil
  transport.call = function(_, method, _, _, done)
    if method == "GetCapabilities" then
      done(nil, { apiVersion = "1", capabilities = { "notes" } })
    else
      identity_calls = identity_calls + 1
    end
  end
  rpc.bootstrap(function(err)
    result = err
  end)
  assert(result and result.message:find("update the LGTM CLI/backend", 1, true))
  assert(
    identity_calls == 0 and not rpc.identity,
    "an old v1 backend is rejected before identity setup"
  )
  transport.call = function(_, method, _, _, done)
    if method == "GetCapabilities" then
      done(nil, { apiVersion = "1", capabilities = rpc.required_capabilities })
    else
      identity_calls = identity_calls + 1
      done(nil, { actor = { id = "supported-backend-actor" } })
    end
  end
  rpc.bootstrap(function(err)
    result = err
  end)
  assert(
    not result and identity_calls == 1 and rpc.identity == "supported-backend-actor",
    "a failed compatibility check does not leave bootstrap stuck"
  )
  transport.call, rpc.identity, rpc.capabilities = old_call, old_identity, old_caps
  require("lgtm.config").options.identity = old_config
  local contexts = require("lgtm.context")
  local previous_state, previous_rpc = contexts.current, rpc.call
  local notifications = require("lgtm.notifications")
  local previous_error, message = notifications.error, nil
  notifications.error = function(value)
    message = value
  end
  local legacy = {
    id = "legacy",
    version = "0",
    state = "completed",
    target = { localGit = { path = "/missing/legacy", branch = "refs/heads/old" } },
  }
  rpc.call = function(method, _, done)
    if method == "GetReview" then
      done(nil, { review = legacy })
    elseif method == "GetConversation" then
      done(nil, { version = "0", comments = {}, threads = {} })
    else
      error("Unavailable source evidence must not invoke content operations: " .. method)
    end
  end
  local state = contexts.apply({}, legacy)
  require("lgtm.review").open(state)
  assert(
    state.conversation and not state.panes,
    "unknown legacy comparisons keep discussion access without guessing content"
  )
  assert(message:find("no recorded comparison", 1, true))
  local previous_caps = rpc.capabilities
  rpc.capabilities = {}
  contexts.set({ kind = "pr", review = { id = "rejected-upgrade", submissionStatus = "failed" } })
  rpc.call = function()
    error("An older backend's recovery method may publish; never invoke it automatically")
  end
  require("lgtm.review").submit()
  assert(
    message:find("Update the LGTM CLI/backend", 1, true),
    "a plugin-only update cannot automatically publish an old rejected outcome"
  )
  require("lgtm.review").reconcile()
  assert(
    message:find("Update the LGTM CLI/backend", 1, true),
    "failed-recovery semantics require explicit backend support"
  )
  rpc.capabilities = previous_caps
  notifications.error = previous_error
  rpc.call = previous_rpc
  contexts.set(previous_state)
  print("Plugin contract checks passed")
end

should_validate_plugin_contracts()
assert(loadfile(root .. "/tests/anchors.lua"))()(root)
assert(loadfile(root .. "/tests/sources.lua"))()(root)
vim.cmd("qa!")
