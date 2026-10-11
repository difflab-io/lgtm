local M = {}
local protocol = require("lgtm.protocol")
local processes = {}
local function notify(message) vim.notify(message, vim.log.levels.ERROR, { title = 'LGTM' }) end
function M.ensure(root, config, callback)
  if config.endpoint then callback({ root = root, endpoint = config.endpoint }); return end
  if processes[root] then
    local state = processes[root]
    if state.endpoint then callback(state) else table.insert(state.waiting, callback) end
    return
  end
  local cli = config.cli or (config.project .. '/bin/lgtm')
  local ui = config.ui or (config.project .. '/apps/lgtm-web-app/build')
  if vim.fn.executable(cli) ~= 1 or vim.fn.filereadable(ui .. '/index.html') ~= 1 then
    vim.notify('Building LGTM for first launch…', vim.log.levels.INFO)
    vim.system({ 'mise', 'run', 'build' }, { cwd = config.project, text = true }, function(result)
      vim.schedule(function()
        if result.code ~= 0 then notify(result.stderr); return end
        M.ensure(root, config, callback)
      end)
    end)
    return
  end
  local state = { root = root, waiting = { callback }, output = '' }
  processes[root] = state
  state.process = vim.system({ cli, 'serve', '--repo', root, '--ui', ui, '--http', '127.0.0.1:0', '--grpc', '127.0.0.1:0' }, {
    text = true,
    stdout = function(_, data)
      if not data then return end
      state.output = state.output .. data
      local url = state.output:match('web (http://[%d%.]+:%d+)')
      if url and not state.endpoint then
        state.endpoint = url .. '/api'; state.url = url
        vim.schedule(function()
          for _, ready in ipairs(state.waiting) do ready(state) end
          state.waiting = {}
        end)
      end
    end,
  }, function(result)
    processes[root] = nil
    if result.code ~= 0 and result.code ~= 130 and result.signal ~= 2 then
      vim.schedule(function() notify(result.stderr ~= '' and result.stderr or 'LGTM service stopped unexpectedly') end)
    end
  end)
end
function M.request(context, payload, callback, onerror)
  if not context then notify('Launch :lgtm first'); return end
  local path, message = protocol.request(payload)
  local url = context.endpoint:gsub('/api$', '') .. path
  vim.system({ 'curl', '--silent', '--show-error', '--max-time', '30', '-H', 'Content-Type: application/json', '-H', 'Connect-Protocol-Version: 1', '--data-binary', vim.json.encode(message), url }, { text = true }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then notify(result.stderr); if onerror then onerror(result.stderr) end; return end
      local ok, out = pcall(vim.json.decode, result.stdout)
      if not ok or type(out) ~= 'table' or out.error or out.code then
        local message = ok and type(out) == 'table' and (out.error or out.message) or 'Invalid LGTM response'
        notify(message); if onerror then onerror(message) end; return
      end
      if callback then callback(protocol.response(out)) end
    end)
  end)
end
function M.stop()
  for _, state in pairs(processes) do if state.process then state.process:kill(2) end end
end
return M
