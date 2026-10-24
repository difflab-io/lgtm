-- State -----------------------------------------------------------------------

local M = {}
-- Public ----------------------------------------------------------------------

function M.run(scope)
  local function export(state)
    require("lgtm.services.reviews").get_export(state, function(err, markdown)
      if err then
        require("lgtm.notifications").error(err)
        return
      end
      require("lgtm.picker").select(
        "Export " .. (scope or "review"),
        { "Preview", "Save file", "Copy to clipboard" },
        tostring,
        function(action)
          if action == "Save file" then
            require("lgtm.picker").input("Export file: ", "review.md", function(path)
              local function write()
                local ok, error =
                  pcall(vim.fn.writefile, vim.split(markdown, "\n", { plain = true }), path)
                if ok then
                  require("lgtm.notifications").show("Export saved: " .. path)
                else
                  require("lgtm.notifications").error(error)
                end
              end
              if vim.fn.filereadable(path) == 1 then
                vim.ui.select(
                  { "Cancel", "Overwrite" },
                  { prompt = "File exists" },
                  function(choice)
                    if choice == "Overwrite" then
                      write()
                    end
                  end
                )
              else
                write()
              end
            end)
          elseif action == "Copy to clipboard" then
            local ok, error = pcall(vim.fn.setreg, "+", markdown)
            if ok then
              require("lgtm.notifications").show("Export copied")
            else
              require("lgtm.notifications").error(error)
            end
          else
            local buf = vim.api.nvim_create_buf(false, true)
            vim.api.nvim_buf_set_lines(
              buf,
              0,
              -1,
              false,
              vim.split(markdown, "\n", { plain = true })
            )
            vim.bo[buf].filetype = "lgtm"
            vim.bo[buf].syntax = "markdown"
            vim.bo[buf].modifiable = false
            vim.cmd("botright split")
            vim.api.nvim_win_set_buf(0, buf)
            vim.keymap.set("n", "q", "<cmd>close<CR>", { buffer = buf })
          end
        end
      )
    end)
  end
  if scope == "notes" then
    require("lgtm.notes").get_existing(vim.api.nvim_get_current_buf(), function(err, state)
      if err then
        require("lgtm.notifications").error(err)
      else
        export(state)
      end
    end)
  else
    local state = require("lgtm.services.state").current
    if state then
      export(state)
    else
      require("lgtm.notifications").error("Select a review first")
    end
  end
end
return M
