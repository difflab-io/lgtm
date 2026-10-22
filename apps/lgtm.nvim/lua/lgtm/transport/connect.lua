-- Unary Connect JSON. No shell interpolation and no write retry after an ambiguous response.
local M = {}
local jobs = {}
function M.endpoint()
  local configured = require("lgtm.config").options.endpoint_url or vim.env.LGTM_ENDPOINT_URL
  if configured and configured ~= "" then
    return configured:gsub("/$", "")
  end
  local home = vim.env.LGTM_HOME or (vim.env.HOME .. "/.lgtm")
  local ok, lines = pcall(vim.fn.readfile, home .. "/server.port")
  local port = ok and tonumber(lines[1])
  if not port or port < 1 or port > 65535 then
    return nil,
      "Cannot discover backend. Run lgtm up --no-open, or set setup({ endpoint_url = 'http://127.0.0.1:PORT' })."
  end
  return "http://127.0.0.1:" .. port .. "/rpc"
end

function M.call(service, method, input, identity, callback)
  local endpoint, err = M.endpoint()
  if not endpoint then
    vim.schedule(function()
      callback({ code = "unavailable", message = err })
    end)
    return function() end
  end
  local config = require("lgtm.config").options
  local args = {
    "curl",
    "--silent",
    "--show-error",
    "--max-time",
    tostring(config.timeout),
    "--max-filesize",
    tostring(config.max_response_bytes),
    "--request",
    "POST",
    "--header",
    "Content-Type: application/json",
    "--header",
    "Connect-Protocol-Version: 1",
    "--data-binary",
    "@-",
    "--write-out",
    "\n%{http_code}",
    endpoint .. "/lgtm.v1." .. service .. "/" .. method,
  }
  if identity then
    table.insert(args, 2, "--header")
    table.insert(args, 3, "x-lgtm-identity: " .. identity)
  end
  local cancelled, job = false, nil
  local ok, result = pcall(vim.system, args, {
    text = true,
    stdin = vim.json.encode((not input or vim.tbl_isempty(input)) and vim.empty_dict() or input),
  }, function(response)
    vim.schedule(function()
      jobs[job] = nil
      if cancelled then
        return
      end
      if response.code ~= 0 then
        callback({
          code = "unavailable",
          message = "Backend request failed: " .. (response.stderr or ""),
          uncertain = true,
        })
        return
      end
      local body, status = (response.stdout or ""):match("^(.*)\n(%d%d%d)$")
      local decoded, data = pcall(vim.json.decode, body or "")
      if not decoded then
        callback({
          code = "invalid_response",
          message = "Invalid Connect JSON response",
          uncertain = true,
        })
        return
      end
      if tonumber(status) ~= 200 then
        callback(data)
        return
      end
      callback(nil, data)
    end)
  end)
  if not ok then
    vim.schedule(function()
      callback({ code = "unavailable", message = tostring(result) })
    end)
    return function() end
  end
  job = result
  local function cancel()
    cancelled = true
    jobs[job] = nil
    job:kill(15)
  end
  jobs[job] = cancel
  return cancel
end

function M.cancel_all()
  local active = jobs
  jobs = {}
  for _, cancel in pairs(active) do
    cancel()
  end
end
return M
