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
	H.scenario("Notifications, cards and modals inherit the theme and accept user highlight overrides", function()
		local namespace = vim.api.nvim_get_hl_ns({ winid = notification_win })
		assert(
			namespace == vim.api.nvim_create_namespace("lgtm.notification-theme"),
			"LGTM styles only its own notification window"
		)
		local accent
		for _, mark in
			ipairs(
				vim.api.nvim_buf_get_extmarks(
					vim.api.nvim_win_get_buf(notification_win),
					require("notify.render.base").namespace(),
					0,
					-1,
					{ details = true }
				)
			)
		do
			local name = mark[4].hl_group
			if name and vim.api.nvim_get_hl(namespace, { name = name, link = true }).link == "LgtmInfo" then
				accent = name
			end
		end
		assert(accent, "notification renderer uses the shared info color")
		vim.api.nvim_set_hl(0, "LgtmInfo", { link = "Special" })
		require("lgtm.cards").highlights()
		assert(
			vim.api.nvim_get_hl(0, { name = "LgtmCommentStatus", link = false }).fg
				== vim.api.nvim_get_hl(0, { name = "Special", link = false }).fg,
			"cards follow an overridden semantic color"
		)
		assert(
			vim.api.nvim_get_hl(namespace, { name = accent, link = true }).link == "LgtmInfo",
			"active notifications retain the theme link"
		)
		local options = require("lgtm.config").options.ui
		local previous = options.border
		options.border = "single"
		local composer = require("lgtm.composer").open("Theme example", "", function(_, done)
			done()
		end)
		assert(vim.api.nvim_win_get_config(composer.win).border[1] == "┌", "modal honors border override")
		local card = require("lgtm.cards").render(
			{ anchor = { kind = "line" }, state = "open" },
			{ { body = "Preview", author = { name = "Reviewer" } } },
			"GitHub draft",
			40,
			false
		)
		assert(card[1][1][1]:sub(1, #"┌") == "┌", "cards share the modal border style")
		assert(
			vim.wo[composer.win].winhighlight:find("FloatBorder:LgtmBorder", 1, true),
			"modal uses shared theme roles"
		)
		H.keys("q")
		options.border = previous
		vim.api.nvim_set_hl(0, "LgtmInfo", { link = "DiagnosticInfo" })
	end)
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
