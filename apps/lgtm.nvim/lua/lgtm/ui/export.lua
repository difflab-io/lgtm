-- State -----------------------------------------------------------------------

local M = {}
local UI, Private = {}, {}
local picker = require("lgtm.ui.picker")
local notice = require("lgtm.ui.notifications")

-- Render ----------------------------------------------------------------------

function M.open(markdown, title)
  UI.actions(markdown, title)
end

-- UI --------------------------------------------------------------------------

function UI.actions(markdown, title)
  picker.select(
    "Export " .. title,
    { "Preview", "Save file", "Copy to clipboard" },
    tostring,
    function(action)
      if action == "Save file" then
        UI.destination(markdown)
      elseif action == "Copy to clipboard" then
        Private.copy(markdown)
      else
        UI.preview(markdown)
      end
    end
  )
end

function UI.destination(markdown)
  picker.input("Export file: ", "review.md", function(path)
    if vim.fn.filereadable(path) == 0 then
      Private.write(markdown, path)
      return
    end
    vim.ui.select({ "Cancel", "Overwrite" }, { prompt = "File exists" }, function(choice)
      if choice == "Overwrite" then
        Private.write(markdown, path)
      end
    end)
  end)
end

function UI.preview(markdown)
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(markdown, "\n", { plain = true }))
  vim.bo[buf].filetype = "lgtm"
  vim.bo[buf].syntax = "markdown"
  vim.bo[buf].modifiable = false
  vim.cmd("botright split")
  vim.api.nvim_win_set_buf(0, buf)
  vim.keymap.set("n", "q", "<cmd>close<CR>", { buffer = buf })
end

-- Private ---------------------------------------------------------------------

function Private.write(markdown, path)
  local ok, err = pcall(vim.fn.writefile, vim.split(markdown, "\n", { plain = true }), path)
  if ok then
    notice.show("Export saved: " .. path)
  else
    notice.error(err)
  end
end

function Private.copy(markdown)
  local ok, err = pcall(vim.fn.setreg, "+", markdown)
  if ok then
    notice.show("Export copied")
  else
    notice.error(err)
  end
end
return M
