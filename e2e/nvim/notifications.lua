-- tests -----------------------------------------------------------------------
local function should_render_fixed_size_notifications(H)
	require("lgtm.notifications").show("Saved UI evidence")
	local notification_win
	H.wait(function()
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			if
				vim.bo[buf].filetype == "notify"
				and table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n"):find("Saved UI evidence", 1, true)
			then
				notification_win = win
				return true
			end
		end
	end, "real nvim-notify rendered message")
	assert(vim.api.nvim_win_get_width(notification_win) == 50, "fixed-width notification")
	assert(vim.api.nvim_win_get_height(notification_win) >= 1 and vim.api.nvim_win_get_height(notification_win) <= 4)
	local picker = require("lgtm.picker").searchable("Cancellation", { "one", "two" }, tostring, function()
		error("cancelled picker selected an item")
	end)
	H.manual_picker = true
	H.keys("not present")
	H.wait(function()
		return #picker.filtered == 0
	end, "picker empty-result state")
	assert(vim.api.nvim_buf_get_lines(picker.list_buf, 0, 1, false)[1] == "(no matches)")
	H.keys("<Esc>")
	assert(picker.closed)
	H.manual_picker = false
end

return should_render_fixed_size_notifications
