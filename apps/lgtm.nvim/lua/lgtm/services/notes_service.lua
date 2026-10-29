-- State -----------------------------------------------------------------------

local M = {}
local contexts = require("lgtm.services.state")
local sources = require("lgtm.services.diff_service")

-- Methods ---------------------------------------------------------------------

function M.get_context(buf)
  local path = vim.api.nvim_buf_get_name(buf)
  if path == "" or vim.bo[buf].buftype ~= "" then
    return nil, { message = "Open a saved file to use notes" }
  end
  local state = contexts.notes[buf]
  if not state or state.scope.notes.path ~= (vim.uv.fs_realpath(path) or path) then
    state = contexts.create_notes(path)
    contexts.notes[buf] = state
    state.observe_sources = function(done)
      require("lgtm.services.threads_service").observe_sources(state, { path = state.path }, done)
    end
    state.refresh = function(done)
      state.read_callbacks = state.read_callbacks or {}
      if done then
        state.read_callbacks[#state.read_callbacks + 1] = done
      end
      state.read_generation = (state.read_generation or 0) + 1
      local generation = state.read_generation
      sources.get_selected_source(state, { path = state.path }, "new", function(err, selected)
        if
          contexts.notes[buf] ~= state
          or not vim.api.nvim_buf_is_valid(buf)
          or generation ~= state.read_generation
        then
          return
        end
        local views = {
          new = vim.tbl_extend("force", selected or {}, {
            path = state.path,
            logical_path = state.path,
            side = "new",
            unavailable = err and err.message,
          }),
        }
        if selected and selected.source then
          local mismatch = require("lgtm.services.anchor_service").check_buffer(views.new, buf)
          if mismatch then
            views.new.unavailable = mismatch.message
          end
        end
        if not vim.deep_equal(state.views, views) then
          state.views = views
        end
        local function finish(error)
          if generation ~= state.read_generation then
            return
          end
          local callbacks = state.read_callbacks
          state.read_callbacks = {}
          for _, waiter in ipairs(callbacks) do
            waiter(error)
          end
        end
        if err then
          finish(err)
          return
        end
        require("lgtm.services.threads_service").get_conversation(state, function(placement_error)
          if placement_error then
            finish(placement_error)
          else
            require("lgtm.services.threads_service").observe_sources(
              state,
              { path = state.path },
              finish
            )
          end
        end)
      end)
    end
  end
  return state
end

function M.get_notes(buf, callback)
  local state, err = M.get_context(buf)
  if not state then
    callback(err)
    return
  end
  state.refresh(function(read_error)
    callback(read_error, state)
  end)
end
function M.invalidate(state)
  state.views = {}
  state.read_generation = (state.read_generation or 0) + 1
end

return M
