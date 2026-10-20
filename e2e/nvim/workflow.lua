-- tests -----------------------------------------------------------------------
local function should_review_notes_directories_and_working_changes(H)
	local root = vim.env.LGTM_E2E_WORKSPACE
	local api = vim.api
	H.scenario("Browse a file, add a range note, manage replies, and export notes", function()
		vim.cmd.edit(vim.fn.fnameescape(root .. "/notes.txt"))
		api.nvim_win_set_cursor(0, { 2, 0 })
		vim.cmd("1,2Lgtm note")
		H.compose("Original standalone note")
		local state = require("lgtm.notes").states[api.nvim_get_current_buf()]
		assert(state and #state.conversation.comments == 1)
		assert(
			tonumber(state.conversation.threads[1].anchor.start) == 1
				and tonumber(state.conversation.threads[1].anchor["end"]) == 2
		)
		assert(#api.nvim_buf_get_extmarks(0, require("lgtm.threads").namespace, 0, -1, {}) == 1)
		local origin = api.nvim_get_current_win()
		H.keys("<CR>")
		H.wait(function()
			return require("lgtm.threads").last and api.nvim_get_current_win() ~= origin
		end, "Enter opens collapsed note")
		local popup = require("lgtm.threads").last
		assert(
			api.nvim_win_get_width(popup.win) == 86 and api.nvim_win_get_height(popup.win) == 5,
			"fixed-size collapsed thread popup"
		)
		local bounds = api.nvim_win_get_config(popup.win)
		assert(bounds.row == math.floor((vim.o.lines - bounds.height - 2) / 2), "thread modal vertically centered")
		assert(bounds.col == math.floor((vim.o.columns - bounds.width - 2) / 2), "thread modal horizontally centered")
		api.nvim_win_set_cursor(popup.win, { popup.entries[1].line, 0 })
		H.keys("c")
		H.compose("Note reply")
		H.wait(function()
			return #state.conversation.comments == 2
		end, "reply persisted")
		assert(api.nvim_get_current_win() == origin, "popup restores focus")
		H.keys("<CR>")
		popup = require("lgtm.threads").last
		local thread_text = table.concat(api.nvim_buf_get_lines(popup.buf, 0, -1, false), "\n")
		assert(
			thread_text:find("TOP-LEVEL COMMENT", 1, true) and thread_text:find("└─ REPLY 1", 1, true),
			"thread distinguishes the root and nested reply"
		)
		assert(not thread_text:find("notes.txt", 1, true), "modal does not repeat the background filename")
		H.keys("<Tab>")
		assert(
			api.nvim_win_get_config(popup.win).footer[1][1]:find("Reply 1", 1, true),
			"footer identifies selected reply"
		)
		H.keys("e")
		assert(
			api.nvim_win_get_config(require("lgtm.composer").last.win).title[1][1]:find("Edit reply 1", 1, true),
			"edit title identifies reply target"
		)
		H.compose("Edited note reply")
		assert(
			state.conversation.comments[1].body == "Original standalone note"
				and state.conversation.comments[2].body == "Edited note reply",
			"editing reply preserves root"
		)
		H.keys(" re")
		H.compose("Edited standalone note")
		assert(state.conversation.comments[1].body == "Edited standalone note")
		H.keys("<CR>")
		H.keys("x")
		H.wait(function()
			return state.conversation.threads[1].state == "resolved"
		end, "resolve")
		H.keys("<CR>")
		H.keys("x")
		H.wait(function()
			return state.conversation.threads[1].state ~= "resolved"
		end, "reopen")
		H.keys("<CR>")
		popup = require("lgtm.threads").last
		local reply_row
		for row, value in pairs(popup.rows) do
			if value.comment and value.comment.body == "Edited note reply" then
				reply_row = row
			end
		end
		assert(reply_row)
		api.nvim_win_set_cursor(popup.win, { reply_row, 0 })
		H.choice = "Delete"
		H.keys("dd")
		H.wait(function()
			return #state.conversation.comments == 1
		end, "delete targets selected reply")
		H.choice = "Preview"
		vim.cmd("Lgtm export notes")
		H.wait(function()
			return table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Edited standalone note", 1, true)
				~= nil
		end, "notes export preview")
		vim.cmd.close()
	end)
	H.scenario(
		"Review ignored plan files, navigate, invalidate reviewed marks, and complete the local review",
		function()
			vim.cmd("Lgtm directory " .. root .. "/.difflab/specific-plan")
			H.wait(function()
				local s = require("lgtm.context").current
				return s and s.panes and s.file.path == "PLAN.md"
			end, "ignored symlink directory in real CodeDiff")
			local review = require("lgtm.context").current
			assert(api.nvim_buf_get_lines(review.panes.new.buf, 0, 1, false)[1] == "# Plan")
			assert(review.explorer and api.nvim_win_is_valid(review.explorer.win), "persistent file pane")
			assert(api.nvim_win_get_width(review.explorer.win) == 30, "fixed-size file pane")
			api.nvim_set_current_win(review.panes.new.win)
			H.keys("f")
			assert(not review.explorer.win, "f hides file pane")
			H.keys("f")
			assert(api.nvim_get_current_win() == review.explorer.win)
			api.nvim_win_set_cursor(review.explorer.win, { 2, 0 })
			H.keys("<CR>")
			H.wait(function()
				return review.file.path == "SECOND.md"
			end, "file pane Enter selection")
			H.keys("[f")
			H.wait(function()
				return review.file.path == "PLAN.md"
			end, "previous file navigation")
			H.keys(" rm")
			assert(review.reviewed["PLAN.md"])
			H.keys(" rm")
			assert(review.reviewed["PLAN.md"] == nil, "second toggle clears reviewed state")
			H.keys(" rm")
			assert(review.reviewed["PLAN.md"])
			api.nvim_set_current_win(review.explorer.win)
			api.nvim_win_set_cursor(review.explorer.win, { 2, 0 })
			H.keys("m")
			assert(review.reviewed["SECOND.md"], "m marks selected directory file")
			assert(api.nvim_win_get_cursor(review.explorer.win)[1] == 2, "mark keeps tree selection")
			H.keys("m")
			assert(review.reviewed["SECOND.md"] == nil, "m clears selected directory mark")
			api.nvim_set_current_win(review.panes.new.win)
			H.keys("<Tab>")
			H.wait(function()
				return review.file.path == "SECOND.md"
			end, "next unreviewed file")
			H.keys("]f")
			H.wait(function()
				return review.file.path == "PLAN.md"
			end, "all-file navigation includes reviewed")
			vim.fn.writefile({ "# Plan", "Review ignored content changed" }, root .. "/.difflab/specific-plan/PLAN.md")
			vim.cmd("Lgtm refresh")
			H.wait(function()
				return review.reviewed["PLAN.md"] == nil
					and review.render_ready
					and review.file.contentHash == review.files[1].contentHash
			end, "changed content invalidates reviewed mark")

			api.nvim_set_current_win(review.panes.new.win)
			api.nvim_win_set_cursor(0, { 2, 0 })
			H.keys(" rc")
			H.compose("Directory line review")
			H.wait(function()
				return #(review.conversation.comments or {}) == 1
			end, "directory line persisted")
			assert(review.conversation.threads[1].anchor.path == "PLAN.md")
			assert(review.conversation.threads[1].anchor.side == "new")
			assert(#api.nvim_buf_get_extmarks(review.panes.new.buf, require("lgtm.threads").namespace, 0, -1, {}) == 1)
			H.keys("za")
			local marks = api.nvim_buf_get_extmarks(
				review.panes.new.buf,
				require("lgtm.threads").namespace,
				0,
				-1,
				{ details = true }
			)
			assert(marks[1][4].virt_lines and #marks[1][4].virt_lines > 0, "thread folds expand")
			H.keys("za")
			H.keys("<CR>")
			H.keys("q")
			assert(api.nvim_get_current_win() == review.panes.new.win)
			vim.cmd("Lgtm comment file")
			H.compose("Directory file review")
			vim.cmd("Lgtm comment general")
			H.compose("Directory general review")
			assert(#review.conversation.comments == 3)
			vim.cmd("Lgtm threads")
			assert(#api.nvim_buf_get_lines(0, 0, -1, false) > 5)
			H.keys("q")
			vim.cmd("Lgtm save")
			H.wait(function()
				for _, entry in ipairs(require("lgtm.notifications").history) do
					if entry.message:find("Review saved", 1, true) then
						return true
					end
				end
			end, "save notification")
			vim.cmd("Lgtm complete")
			H.wait(function()
				return review.review.state == "completed"
			end, "local completion")
			vim.cmd("Lgtm close")
			assert(require("lgtm.context").current == nil)
		end
	)
	H.scenario("Review working changes with old-side comments and switch CodeDiff layouts", function()
		H.choice = "working"
		vim.cmd("Lgtm review")
		H.wait(function()
			local active = require("lgtm.context").current
			return active and active.kind == "git" and active.panes
		end, "working Git comparison")
		local branch = require("lgtm.context").current
		assert(branch.base)
		vim.api.nvim_set_current_win(branch.panes.old.win)
		vim.api.nvim_win_set_cursor(0, { 2, 0 })
		vim.cmd("Lgtm comment line")
		H.compose("Branch old-side feedback\n\n- Preserve **Markdown** and `code`.")
		assert(branch.conversation.threads[1].anchor.side == "old")
		H.keys("<CR>")
		H.keys("c")
		H.compose("Old-side reply with a useful alternative")
		H.wait(function()
			return #branch.conversation.comments == 2
		end, "old-side reply saved")
		vim.cmd("Lgtm layout")
		H.wait(function()
			return branch.layout == "inline"
		end, "actual CodeDiff inline layout")
		vim.cmd("Lgtm layout")
		H.wait(function()
			return branch.layout == "side-by-side"
		end, "restore side-by-side layout")
	end)
	H.scenario("Promote a standalone note into a review and export it to a file and clipboard", function()
		local branch = require("lgtm.context").current
		vim.cmd("tabnew")
		local notes_tab = vim.api.nvim_get_current_tabpage()
		vim.cmd.edit(vim.fn.fnameescape(root .. "/notes.txt"))
		H.choices = {
			function(items)
				return items[1]
			end,
			"Promote",
		}
		vim.cmd("Lgtm promote")
		H.wait(function()
			return #branch.conversation.comments == 3
		end, "note promotion with confirmed target anchor")
		H.choices = nil
		assert(vim.api.nvim_get_current_tabpage() == branch.tab, "promotion shows the selected target review")
		vim.cmd(vim.api.nvim_tabpage_get_number(notes_tab) .. "tabclose")
		local before_export = vim.deepcopy(branch.conversation)
		H.choice = "Save file"
		vim.cmd("Lgtm export review")
		H.wait(function()
			return vim.fn.filereadable(root .. "/review.md") == 1
		end, "review export file")
		assert(table.concat(vim.fn.readfile(root .. "/review.md"), "\n"):find("Branch old-side feedback", 1, true))
		local copied
		vim.g.clipboard = {
			name = "E2E external clipboard",
			copy = {
				["+"] = function(lines)
					copied = table.concat(lines, "\n")
				end,
				["*"] = function() end,
			},
			paste = {
				["+"] = function()
					return { {}, "v" }
				end,
				["*"] = function()
					return { {}, "v" }
				end,
			},
		}
		H.choice = "Copy to clipboard"
		vim.cmd("Lgtm export review")
		H.wait(function()
			return copied ~= nil
		end, "review clipboard export")
		assert(copied:find("Edited standalone note", 1, true), "promoted note attribution/body exported")
		assert(copied:find("1. **[COMMENT]**", 1, true), "numbered reader-facing feedback")
		assert(copied:find(":~2", 1, true) and copied:find("old (left) side", 1, true), "old-side location and legend")
		assert(copied:find("- Preserve **Markdown** and `code`.", 1, true), "multiline Markdown retained")
		assert(copied:find("**Reply", 1, true) and copied:find("useful alternative", 1, true), "nested reply retained")
		for _, comment in ipairs(branch.conversation.comments) do
			assert(not copied:find(comment.id, 1, true), "no database IDs in feedback")
		end
		assert(
			vim.deep_equal(before_export, branch.conversation),
			"file and clipboard exports do not mutate saved feedback"
		)
		vim.cmd("Lgtm close")
	end)
end

return should_review_notes_directories_and_working_changes
