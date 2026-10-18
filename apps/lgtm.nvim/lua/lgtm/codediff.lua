-- The only adapter to pinned CodeDiff internals. Content always comes from the backend.
local M = {}
local api = vim.api
function M.open(state, file)
  state.render_generation = (state.render_generation or 0) + 1
  local generation = state.render_generation
  state.render_ready = false
  local ok, view = pcall(require, "codediff.ui.view")
  if not ok then
    require("lgtm.notifications").error(
      "Install esmuellert/codediff.nvim and its native library; see :checkhealth lgtm"
    )
    return
  end
  state.temp_dir = state.temp_dir or vim.fn.tempname()
  vim.fn.mkdir(state.temp_dir, "p")
  local token = vim.fn.sha256(file.path .. (file.contentHash or file.content or "")):sub(1, 16)
  local extension = file.path:match("(%.[^./]+)$") or ""
  local original, modified =
    state.temp_dir .. "/" .. token .. "-old" .. extension,
    state.temp_dir .. "/" .. token .. "-new" .. extension
  vim.fn.writefile(vim.split(file.baseContent or "", "\n", { plain = true }), original)
  vim.fn.writefile(vim.split(file.content or "", "\n", { plain = true }), modified)
  local paths = require("codediff.core.path")
  local config = {
    original = paths.make_ref(original),
    modified = paths.make_ref(modified),
    layout = "side-by-side",
  }
  local function ready()
    if generation ~= state.render_generation then
      return
    end
    local lifecycle = require("codediff.ui.lifecycle")
    local session = lifecycle.get_session(state.tab or api.nvim_get_current_tabpage())
    if not session then
      return
    end
    state.tab = api.nvim_win_get_tabpage(session.modified_win)
    state.bind_panes = ready
    state.layout = session.layout
    state.panes = {
      old = { buf = session.original_bufnr, win = session.original_win },
      new = { buf = session.modified_bufnr, win = session.modified_win },
    }
    state.file = file
    state.render = function()
      require("lgtm.files").render(state)
      for side, pane in pairs(state.panes) do
        require("lgtm.threads").render(state, pane.buf, file.path, side)
      end
    end
    for side, pane in pairs(state.panes) do
      vim.bo[pane.buf].modifiable = false
      vim.b[pane.buf].lgtm = { path = file.path, side = side, content_hash = file.contentHash }
      require("lgtm.threads").attach(state, pane.buf, file.path, side)
      require("lgtm.files").attach(state, pane.buf)
      vim.keymap.set(
        { "n", "x" },
        require("lgtm.config").options.prefix .. "C",
        vim.schedule_wrap(function()
          require("lgtm.review").comment("line")
        end),
        { buffer = pane.buf, desc = "Add review comment" }
      )
      if api.nvim_win_is_valid(pane.win) then
        vim.wo[pane.win].winbar = "LGTM · " .. side .. " · " .. file.path
      end
    end
    if state.explorer_visible ~= false then
      require("lgtm.files").show(state, false)
    end
    require("lgtm.files").render(state)
    if state.focus_side then
      local pane = state.panes[state.focus_side]
      state.focus_side = nil
      if pane and api.nvim_win_is_valid(pane.win) then
        api.nvim_set_current_win(pane.win)
      end
    end
    state.render_ready = true
    if state.on_ready then
      state.on_ready()
    end
  end
  local deadline = vim.uv.hrtime() + 5e9
  local function wait_ready()
    if generation ~= state.render_generation then
      return
    end
    local session = state.tab and require("codediff.ui.lifecycle").get_session(state.tab)
    if
      session
      and session.stored_diff_result
      and (not session.refresh or not session.refresh.loading)
    then
      ready()
    elseif vim.uv.hrtime() < deadline then
      vim.defer_fn(wait_ready, 10)
    else
      require("lgtm.notifications").error("CodeDiff did not finish loading this file")
    end
  end
  state.wait_ready = function()
    deadline = vim.uv.hrtime() + 5e9
    wait_ready()
  end
  if state.tab and api.nvim_tabpage_is_valid(state.tab) then
    api.nvim_set_current_tabpage(state.tab)
    view.update(state.tab, config, false)
    vim.schedule(wait_ready)
  else
    local result = view.create(config, vim.filetype.match({ filename = file.path }) or "", ready)
    if result then
      state.tab = api.nvim_win_get_tabpage(result.modified_win)
    end
  end
end

function M.toggle(state)
  if not state or not state.tab then
    return
  end
  if require("codediff.ui.view.toggle").toggle(state.tab) then
    vim.schedule(function()
      if state.wait_ready then
        state.wait_ready()
      end
    end)
  end
end

function M.close(state)
  if state.tab and api.nvim_tabpage_is_valid(state.tab) then
    pcall(api.nvim_set_current_tabpage, state.tab)
    if #api.nvim_list_tabpages() > 1 then
      vim.cmd("tabclose")
    end
  end
  if state.explorer and state.explorer.buf and api.nvim_buf_is_valid(state.explorer.buf) then
    pcall(api.nvim_buf_delete, state.explorer.buf, { force = true })
  end
  if state.temp_dir then
    vim.fn.delete(state.temp_dir, "rf")
  end
end
return M
