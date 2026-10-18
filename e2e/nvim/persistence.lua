-- tests -----------------------------------------------------------------------
local function should_support_persistence(H)
	vim.cmd.edit(vim.fn.fnameescape(vim.env.LGTM_E2E_WORKSPACE .. "/notes.txt"))
	H.wait(function()
		local state = require("lgtm.notes").states[vim.api.nvim_get_current_buf()]
		return state
			and state.conversation
			and #vim.api.nvim_buf_get_extmarks(0, require("lgtm.threads").namespace, 0, -1, {}) > 0
	end, "persisted notes appear on file entry")
	vim.cmd("Lgtm notes")
	H.wait(function()
		return require("lgtm.threads").last ~= nil
	end, "notes after editor restart")
	assert(table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Edited standalone note", 1, true))
	H.keys("q")
	local sessions = H.rpc("ListSessions", { completed = true })
	local found
	for _, entry in ipairs(sessions.entries or {}) do
		if entry.context.key.path and entry.context.key.path:find("specific%-plan") then
			found = entry
		end
	end
	assert(found, "completed local session remains selectable")
	local conversation = H.rpc("GetConversation", { contextId = found.context.id, includeDrafts = true })
	assert(#conversation.comments == 3)
	local export = H.rpc("Export", { sessionId = found.session.id })
	assert(export.markdown:find("Directory line review", 1, true))
	assert(export.markdown:find("Directory file review", 1, true))
	assert(export.markdown:find("Directory general review", 1, true))
	H.choice = function(items)
		for _, entry in ipairs(items) do
			if entry.context.key.path and entry.context.key.path:find("specific%-plan") then
				return entry
			end
		end
	end
	H.manual_picker = true
	vim.cmd("Lgtm sessions completed")
	H.wait(function()
		local p = require("lgtm.picker").last
		return p and not p.closed
	end, "real session search picker")
	local picker = require("lgtm.picker").last
	H.keys("specific-plan")
	H.wait(function()
		return #picker.filtered == 1
	end, "local sessions filter by typed query")
	assert(vim.api.nvim_win_get_width(picker.win) == 76 and vim.api.nvim_win_get_height(picker.list_win) == 10)
	H.choice = nil
	H.keys("<CR>")
	H.manual_picker = false
	H.wait(function()
		local state = require("lgtm.context").current
		return state and state.session.id == found.session.id and state.panes
	end, "completed local-session picker")
	vim.cmd("Lgtm close")
	local before = H.rpc("ListSessions", { completed = true })
	vim.cmd("tabnew")
	vim.cmd.edit(vim.fn.fnameescape(vim.env.LGTM_E2E_WORKSPACE .. "/.difflab/specific-plan/PLAN.md"))
	H.choice = "Preview"
	vim.cmd("Lgtm export notes")
	H.wait(function()
		return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Directory line review", 1, true)
	end, "export completed notes without successor")
	local after = H.rpc("ListSessions", { completed = true })
	assert(#before.entries == #after.entries, "notes export never creates a successor")
	assert(H.rpc("GetSession", { sessionId = found.session.id }).session.state == "completed")
	vim.cmd("close")
	vim.cmd("tabclose")
end

return should_support_persistence
