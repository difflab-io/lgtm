local M = { drafts = {} }

-- Persistence is supplied by the feature. Retained text does not own a hidden window.
function M.open(title, initial, submit, options)
  local theme = require("lgtm.ui.theme")
  if not theme.available() then
    return
  end
  local n = require("nui-components")
  local Input = n.text_input:extend("LgtmDraftInput")
  function Input:events()
    return {}
  end
  local origin, cursor = vim.api.nvim_get_current_win(), vim.api.nvim_win_get_cursor(0)
  local key = (options and options.draft_id)
    or (vim.api.nvim_get_current_buf() .. ":" .. title .. ":" .. (initial or ""))
  local signal = n.create_signal({ body = M.drafts[key] or initial or "", pending = false })
  local view = { closed = false, title = title }
  local input, renderer, resize
  local function restore()
    if vim.api.nvim_win_is_valid(origin) then
      vim.api.nvim_set_current_win(origin)
      pcall(vim.api.nvim_win_set_cursor, origin, cursor)
    end
  end
  local function size()
    return {
      width = math.max(1, math.min(82, vim.o.columns - 2)),
      height = math.max(3, math.min(14, vim.o.lines - 3)),
    }
  end
  renderer = n.create_renderer(size())
  local function close(saved)
    if view.closed then
      return
    end
    local focused = view.win and vim.api.nvim_get_current_win() == view.win
    view.closed = true
    M.drafts[key] = not saved and signal:get_value().body or nil
    if resize then
      vim.api.nvim_del_autocmd(resize)
      resize = nil
    end
    renderer:close()
    if focused then
      restore()
    end
  end
  local function save()
    if view.closed or signal:get_value().pending then
      return
    end
    local body = table.concat(vim.api.nvim_buf_get_lines(input.bufnr, 0, -1, false), "\n")
    signal.body, signal.pending = body, true
    renderer.layout:hide()
    restore()
    submit(body, function(err)
      signal.pending = false
      if err then
        M.drafts[key] = body
        if not view.closed then
          renderer.layout:show()
          view.win, view.buf = input.winid, input.bufnr
          input:focus()
        end
        require("lgtm.notifications").error(err)
      else
        M.drafts[key] = nil
        close(true)
      end
    end)
  end
  -- The pinned TextInput validates wrap, but linebreak belongs in popup win_options.
  local props = vim.tbl_extend("force", theme.get_component_options(), {
    id = "draft",
    flex = 1,
    autofocus = true,
    filetype = "lgtm",
    wrap = true,
    value = signal.body,
    on_change = function(body)
      if signal:get_value().body ~= body then
        signal.body = body
      end
    end,
    on_unmount = function()
      close(false)
    end,
    on_mount = function(component)
      view.buf, view.win = component.bufnr, component.winid
      vim.bo[view.buf].syntax = "markdown"
    end,
  })
  input = Input(
    props,
    theme.get_popup_options(
      title,
      " <C-s> " .. (options and options.action or "save") .. " · q/Esc cancel "
    )
  )
  renderer:add_mappings({
    { mode = { "n", "i" }, key = "<C-s>", handler = save },
    {
      mode = "n",
      key = "q",
      handler = function()
        close(false)
      end,
    },
  })
  renderer:on_unmount(function()
    if not view.closed then
      local focused = view.win and vim.api.nvim_get_current_win() == view.win
      view.closed = true
      M.drafts[key] = signal:get_value().body
      if resize then
        vim.api.nvim_del_autocmd(resize)
        resize = nil
      end
      if focused then
        restore()
      end
    end
  end)
  resize = vim.api.nvim_create_autocmd("VimResized", {
    callback = function()
      if not view.closed then
        renderer:set_size(size())
      end
    end,
  })
  renderer:render(input)
  view.renderer, view.save, view.close = renderer, save, function()
    close(false)
  end
  M.last = view
  return view
end
return M
