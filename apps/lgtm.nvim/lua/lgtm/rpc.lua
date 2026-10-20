local M = {
  identity = nil,
  capabilities = nil,
  required_capabilities = { "review_targets", "feedback_scopes", "anchor_evidence", "diff_service" },
}
local initializing, waiting = false, {}
function M.bootstrap(callback)
  if M.identity then
    callback()
    return
  end
  waiting[#waiting + 1] = callback
  if initializing then
    return
  end
  initializing = true
  local transport = require("lgtm.transport.connect")
  local function finish(err)
    initializing = false
    local callbacks = waiting
    waiting = {}
    for _, cb in ipairs(callbacks) do
      cb(err)
    end
  end
  transport.call("IdentityService", "GetCapabilities", {}, nil, function(err, result)
    if err then
      finish(err)
      return
    end
    if result.apiVersion ~= "1" and result.apiVersion ~= "v1" then
      finish({ message = "Unsupported backend API: " .. tostring(result.apiVersion) })
      return
    end
    M.capabilities = result.capabilities
    for _, name in ipairs(M.required_capabilities) do
      if not vim.list_contains(result.capabilities or {}, name) then
        finish({
          message = "Backend is missing "
            .. name
            .. "; update the LGTM CLI/backend to match this plugin",
        })
        return
      end
    end
    local identity = require("lgtm.config").options.identity
    if not identity then
      finish({
        message = "Configure lgtm.setup({ identity = { name = 'Name', email = 'you@example.com' } })",
      })
      return
    end
    transport.call("IdentityService", "EnsureIdentity", identity, nil, function(error, response)
      if not error then
        M.identity = response.actor.id
      end
      finish(error)
    end)
  end)
end

function M.call(method, input, callback)
  local cancelled, cancel = false, nil
  M.bootstrap(function(err)
    if cancelled then
      return
    end
    if err then
      callback(err)
      return
    end
    local service = ({ GetDiff = true, GetFiles = true, GetFile = true })[method] and "DiffService"
      or "ReviewService"
    cancel = require("lgtm.transport.connect").call(service, method, input, M.identity, callback)
  end)
  return function()
    cancelled = true
    if cancel then
      cancel()
    end
  end
end
local pending = {}
function M.latest(key, method, input, callback)
  if pending[key] then
    pending[key].cancel()
  end
  local generation = {}
  local cancel = M.call(method, input, function(err, result)
    if pending[key] ~= generation then
      return
    end
    pending[key] = nil
    callback(err, result)
  end)
  generation.cancel = cancel
  pending[key] = generation
  return cancel
end
return M
