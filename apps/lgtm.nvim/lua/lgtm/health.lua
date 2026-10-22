local M = {}
function M.check()
  vim.health.start("lgtm.nvim")
  if vim.fn.has("nvim-0.11") == 1 then
    vim.health.ok("Neovim >= 0.11")
  else
    vim.health.error("Install Neovim >= 0.11")
  end
  for _, binary in ipairs({ "lgtm", "curl" }) do
    if vim.fn.executable(binary) == 1 then
      vim.health.ok(binary .. " available")
    else
      vim.health.error("Install " .. binary .. " and add it to PATH")
    end
  end
  for _, module in ipairs({
    "codediff",
    "codediff.ui.view",
    "codediff.ui.lifecycle",
    "which-key",
    "notify",
  }) do
    if pcall(require, module) then
      vim.health.ok(module .. " available")
    else
      vim.health.error("Missing dependency " .. module .. "; see lgtm.nvim README")
    end
  end
  for module, functions in pairs({
    ["codediff.ui.view"] = { "create", "update" },
    ["codediff.ui.lifecycle"] = { "get_session", "get_panel_view" },
    ["codediff.core.git"] = {
      "get_diff_revision_with_line_stats",
      "get_diff_revisions_with_line_stats",
      "get_diff_staged",
    },
  }) do
    local available, api = pcall(require, module)
    if available then
      for _, name in ipairs(functions) do
        if type(api[name]) ~= "function" then
          vim.health.error(
            "Incompatible CodeDiff API: "
              .. module
              .. "."
              .. name
              .. " missing; use the README's pinned revision"
          )
        end
      end
    end
  end
  local ok, diff = pcall(require, "codediff.core.diff")
  if ok and type(diff.compute_diff) == "function" then
    local loaded, result = pcall(diff.compute_diff, { "before" }, { "after" })
    if loaded and result then
      vim.health.ok("CodeDiff native library loaded")
    else
      vim.health.error(
        "CodeDiff native library unavailable; allow its binary download or build from source"
      )
    end
  end
  local endpoint, err = require("lgtm.transport.connect").endpoint()
  if endpoint then
    local ok, response = pcall(function()
      return vim
        .system({
          "curl",
          "--silent",
          "--show-error",
          "--max-time",
          "3",
          "--connect-timeout",
          "2",
          "--max-filesize",
          "1048576",
          "--header",
          "Content-Type: application/json",
          "--header",
          "Connect-Protocol-Version: 1",
          "--data",
          "{}",
          "--write-out",
          "\n%{http_code}",
          endpoint .. "/lgtm.v1.IdentityService/GetCapabilities",
        }, { text = true })
        :wait()
    end)
    if not ok or response.code ~= 0 then
      vim.health.error(
        "Backend unavailable at " .. endpoint .. "; run lgtm up --no-open or configure endpoint_url"
      )
    else
      local body, status = response.stdout:match("^(.*)\n(%d%d%d)$")
      local decoded, capabilities = pcall(vim.json.decode, body or "")
      if tonumber(status) ~= 200 or not decoded then
        vim.health.error(
          "Backend does not expose Connect JSON at " .. endpoint .. "; check the /rpc daemon prefix"
        )
      elseif capabilities.apiVersion ~= "1" and capabilities.apiVersion ~= "v1" then
        vim.health.error(
          "Incompatible backend API "
            .. tostring(capabilities.apiVersion)
            .. "; rebuild the CLI/backend from this checkout"
        )
      else
        local missing = vim.tbl_filter(function(name)
          return not vim.list_contains(capabilities.capabilities or {}, name)
        end, require("lgtm.rpc").required_capabilities)
        if #missing > 0 then
          vim.health.error(
            "Backend is missing "
              .. table.concat(missing, ", ")
              .. "; update the LGTM CLI/backend to match this plugin"
          )
        else
          vim.health.ok("Backend reachable, API " .. capabilities.apiVersion .. ": " .. endpoint)
        end
      end
    end
  else
    vim.health.error(err)
  end
  if require("lgtm.config").options.identity then
    vim.health.ok("Identity configured")
  else
    vim.health.error("Set setup.identity.name/email (or model/provider for agents)")
  end
  vim.health.info(
    "Git reviews need git. GitHub reviews need gh auth login for the backend's user. The plugin never accesses the web frontend."
  )
end
return M
