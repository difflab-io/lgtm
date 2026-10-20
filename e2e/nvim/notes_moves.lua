-- fixtures --------------------------------------------------------------------
local api = vim.api
local function read_scope(H, scope)
	return H.rpc("GetConversation", { scope = scope })
end
local function notes(H, buf, body)
	api.nvim_set_current_buf(buf)
	api.nvim_win_set_cursor(0, { 2, 0 })
	vim.cmd("Lgtm note")
	H.compose(body)
	return require("lgtm.notes").states[buf]
end

-- tests -----------------------------------------------------------------------
return function(H)
	local workspace = vim.env.LGTM_E2E_WORKSPACE
	local original = workspace .. "/.difflab/move-case"
	vim.fn.mkdir(original, "p")
	original = vim.uv.fs_realpath(original)
	vim.fn.writefile({ "before", "selected", "after" }, original .. "/NOTE.md")
	vim.fn.writefile({ "before", "selected", "after" }, original .. "/PLAN.md")
	local destination = vim.fs.dirname(original) .. "/moved-note.md"
	local renamed = vim.fs.dirname(original) .. "/moved-plan"
	local last = vim.fs.dirname(original) .. "/final-plan"
	local note_state, note_id, note_buf, review, review_id, review_thread_id
	H.scenario(
		"Move a saved note between parents through editor file events and preserve its original evidence",
		function()
			vim.cmd.edit(vim.fn.fnameescape(original .. "/NOTE.md"))
			note_buf = api.nvim_get_current_buf()
			note_state = notes(H, note_buf, "Note before moving")
			note_id = note_state.conversation.comments[1].id
			assert(vim.uv.fs_rename(original .. "/NOTE.md", destination))
			vim.cmd.file(vim.fn.fnameescape(destination))
			H.wait(function()
				return not note_state.moving
					and note_state.scope.notes.path == destination
					and note_state.conversation.comments[1].id == note_id
			end, "editor file rename records the same saved discussion at its new path")
			assert(note_state.conversation.threads[1].sourcePath == original .. "/NOTE.md")
			assert(note_state.conversation.threads[1].currentPath == destination)
			notes(H, note_buf, "New note after moving")
			assert(#note_state.conversation.comments == 2)
			local newer
			for _, thread in ipairs(note_state.conversation.threads) do
				if thread.sourcePath == destination then
					newer = thread
				end
			end
			assert(newer, "new feedback records the actual new source location")
			local copy = original .. "/COPY.md"
			vim.fn.writefile(vim.fn.readfile(destination), copy)
			api.nvim_exec_autocmds("User", { pattern = "LgtmPathRenamed", data = { from = destination, to = copy } })
			assert(note_state.scope.notes.path == destination, "a copy does not move the saved note")
			assert(#(read_scope(H, { notes = { path = copy } }).comments or {}) == 0)
		end
	)
	H.scenario("Move an ignored review directory twice and add feedback after the first root move", function()
		vim.cmd("Lgtm directory " .. original)
		H.wait(function()
			local state = require("lgtm.context").current
			return state and state.kind == "directory" and state.render_ready
		end, "ignored directory review")
		review = require("lgtm.context").current
		review.show_file("PLAN.md")
		H.wait(function()
			return review.render_ready and review.file.path == "PLAN.md"
		end, "selected ignored plan")
		api.nvim_set_current_win(review.panes.new.win)
		api.nvim_win_set_cursor(0, { 2, 0 })
		H.keys("c")
		H.compose("Review before root move")
		review_id = review.conversation.comments[1].id
		review_thread_id = review.conversation.comments[1].threadId
		for index, move in ipairs({ { original, renamed }, { renamed, last } }) do
			assert(vim.uv.fs_rename(move[1], move[2]))
			api.nvim_exec_autocmds("User", {
				pattern = index == 1 and "MiniFilesActionRename" or "LgtmPathRenamed",
				data = { from = move[1], to = move[2] },
			})
			H.wait(function()
				return not review.moving
					and review.root == move[2]
					and review.render_ready
					and vim.iter(review.conversation.comments):any(function(comment)
						return comment.id == review_id
					end)
			end, "confirmed ignored-directory root move " .. index)
			local snapshot = read_scope(H, review.scope)
			local root_thread
			for _, thread in ipairs(snapshot.threads) do
				if thread.id == review_thread_id then
					root_thread = thread
				end
			end
			assert(root_thread and root_thread.anchor.path == "PLAN.md")
			assert(root_thread.sourcePath == original .. "/PLAN.md")
			assert(root_thread.currentPath == move[2] .. "/PLAN.md")
			assert(require("lgtm.threads").placement(review, root_thread, "PLAN.md", "new").status == "exact")
			assert(
				vim.inspect(
					api.nvim_buf_get_extmarks(
						review.panes.new.buf,
						require("lgtm.threads").namespace,
						0,
						-1,
						{ details = true }
					)
				):find("Review before root move", 1, true),
				"moved root comment remains visible in the actual diff"
			)
			if index == 1 then
				api.nvim_set_current_win(review.panes.new.win)
				api.nvim_win_set_cursor(0, { 3, 0 })
				H.keys("c")
				H.compose("Review after first root move")
			end
		end
		assert(#review.conversation.comments == 2)
		local before = read_scope(H, review.scope)
		H.choice = "Preview"
		vim.cmd("Lgtm export review")
		H.wait(function()
			return vim.bo.filetype == "lgtm"
		end, "moved review export")
		local text = table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(text:find("Review before root move", 1, true) and text:find("Review after first root move", 1, true))
		H.keys("q")
		assert(vim.deep_equal(before, read_scope(H, review.scope)), "move-aware export is read only")
		vim.fn.writefile({
			vim.json.encode({
				reviewId = review.review.id,
				root = last,
				notePath = destination,
				noteId = note_id,
				commentId = review_id,
			}),
		}, vim.env.LGTM_HOME .. "/notes-move-proof.json")
		vim.cmd("Lgtm close")
	end)
	vim.cmd.cd(workspace)
end
