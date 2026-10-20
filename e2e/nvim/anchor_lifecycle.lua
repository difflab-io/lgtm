-- fixtures --------------------------------------------------------------------
local api = vim.api
local function git(root, ...)
	local args = {
		"git",
		"-C",
		root,
		"-c",
		"user.name=E2E",
		"-c",
		"user.email=e2e@example.test",
		"-c",
		"core.hooksPath=/dev/null",
	}
	vim.list_extend(args, { ... })
	local result =
		vim.system(args, { text = true, env = { GIT_CONFIG_GLOBAL = "/dev/null", GIT_CONFIG_NOSYSTEM = "1" } }):wait()
	assert(result.code == 0, result.stderr)
	return vim.trim(result.stdout)
end

local function bytes(path, content)
	local fd = assert(vim.uv.fs_open(path, "w", 438))
	assert(vim.uv.fs_write(fd, content, 0))
	assert(vim.uv.fs_close(fd))
end

local function thread(state, id)
	for _, value in ipairs(state.conversation.threads) do
		if value.id == id then
			return value
		end
	end
	error("Saved discussion is missing: " .. id)
end

local function open(H, mode, path, replace)
	local previous = require("lgtm.context").current
	if previous then
		vim.cmd("Lgtm close")
	end
	H.choices = replace and { mode, "Complete saved review and start this comparison" } or { mode }
	vim.cmd("Lgtm review")
	H.wait(function()
		local state = require("lgtm.context").current
		return state and state.review and state.render_ready
	end, "open " .. mode .. " comparison through the real picker")
	local state = require("lgtm.context").current
	assert(
		state.native_explorer and require("codediff.ui.lifecycle").get_panel_name(state.tab) == "explorer",
		"local review uses CodeDiff's native explorer"
	)
	if path and state.file.path ~= path then
		state.show_file(path)
		H.wait(function()
			return state.file.path == path and state.render_ready
		end, "select " .. path .. " in CodeDiff")
	end
	return state
end

local function placement(state, id)
	return require("lgtm.threads").placement(state, thread(state, id), state.file.path, "new")
end

-- tests -----------------------------------------------------------------------
return function(H)
	local rpc = require("lgtm.rpc")
	local call = rpc.call
	rpc.call = function(method, input, callback)
		assert(
			method ~= "GetDiff" and method ~= "GetFiles" and method ~= "GetFile",
			"local editor reviews must not request backend content: " .. method
		)
		return call(method, input, callback)
	end
	local root = vim.env.LGTM_HOME .. "/anchor-repo"
	vim.fn.mkdir(root .. "/src", "p")
	root = vim.uv.fs_realpath(root)
	git(root, "init", "-q", "-b", "main")
	local path, moved = "src/source [literal] ü.txt", "moved/source [literal] ü.txt"
	local tail = {}
	for index = 1, 10 do
		tail[#tail + 1] = "stable context " .. index
	end
	tail[#tail + 1] = "footer"
	local base_text = "header\noriginal\n" .. table.concat(tail, "\n") .. "\n"
	local staged_text = "header\r\nselected\r\n" .. table.concat(tail, "\r\n")
	local relocated_text = "intro\nheader\nselected\n" .. table.concat(tail, "\n") .. "\n"
	bytes(root .. "/" .. path, base_text)
	git(root, "add", "--", path)
	git(root, "commit", "-qm", "base")
	git(root, "checkout", "-qb", "feature")
	bytes(root .. "/" .. path, staged_text)
	git(root, "add", "--", path)
	local oid = git(root, "rev-parse", ":" .. path)
	bytes(root .. "/" .. path, relocated_text)
	vim.cmd.cd(root)
	local state, root_id, reply_id, original, working_id
	H.scenario("Comment on exact staged CRLF bytes while the working copy differs", function()
		state = open(H, "staged", path)
		api.nvim_set_current_win(state.panes.new.win)
		api.nvim_win_set_cursor(0, { 2, 0 })
		H.keys("c")
		H.compose("Preserve this staged selection")
		local saved = state.conversation.comments[1]
		root_id = saved.threadId
		original = vim.deepcopy(thread(state, root_id).anchor)
		assert(
			original.source.indexFile.blobOid == oid,
			"anchor identifies stage zero rather than HEAD or working text"
		)
		assert(original.evidence.selectedText == "selected")
		assert(
			original.source.indexFile.contentHash == "sha256:" .. vim.fn.sha256(staged_text),
			"CRLF and absent final newline are hashed exactly"
		)
		H.keys("<CR>")
		H.keys("c")
		H.compose("Keep this reply with the original discussion")
		for _, comment in ipairs(state.conversation.comments) do
			if tonumber(comment.position) == 1 then
				reply_id = comment.id
			end
		end
		assert(reply_id)
		vim.cmd("Lgtm complete")
		H.wait(function()
			return state.review.state == "completed"
		end, "complete staged attempt")
		git(root, "commit", "-qm", "commit reviewed index")
	end)
	H.scenario("Carry staged comments and replies into a committed branch comparison", function()
		state = open(H, "branch", path)
		local location = placement(state, root_id)
		assert(location and location.location.start == 2, "staged bytes reconnect to their committed source")
		assert(vim.deep_equal(thread(state, root_id).anchor, original), "original index evidence stays immutable")
		assert(vim.iter(state.conversation.comments):any(function(comment)
			return comment.id == reply_id
		end))
		git(root, "add", "--", path)
		git(root, "commit", "-qm", "insert leading line")
		state = open(H, "branch", path, true)
		location = placement(state, root_id)
		assert(
			location and location.status == "relocated" and location.location.start == 3,
			"whole selection and neighboring evidence move to line 3"
		)
		api.nvim_set_current_win(state.panes.new.win)
		api.nvim_win_set_cursor(0, { 3, 0 })
		H.keys("<CR>")
		assert(
			table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Preserve this staged selection", 1, true)
		)
		H.keys("q")
	end)
	H.scenario("Follow a confirmed directory rename without copying threads or rewriting origins", function()
		git(root, "mv", "src", "moved")
		git(root, "commit", "-qm", "move reviewed directory")
		state = open(H, "branch", moved, true)
		H.wait(function()
			return thread(state, root_id).currentPath == moved
		end, "persist confirmed Git rename")
		assert(placement(state, root_id).location.start == 3)
		assert(vim.deep_equal(thread(state, root_id).anchor, original))
		assert(#state.conversation.comments == 2, "move retains one root and one reply")
	end)
	H.scenario("Float a lost selection above the file and open its original evidence in a centered thread", function()
		bytes(root .. "/" .. moved, relocated_text:gsub("selected", "replacement"))
		git(root, "add", "--", moved)
		git(root, "commit", "-qm", "replace reviewed selection")
		state = open(H, "branch", moved, true)
		local lost = placement(state, root_id)
		assert(lost.status == "outdated" and not lost.location, "lost text cannot be clamped to an invented line")
		local marks =
			api.nvim_buf_get_extmarks(state.panes.new.buf, require("lgtm.threads").namespace, 0, -1, { details = true })
		assert(
			#marks == 1 and marks[1][2] == 0 and marks[1][4].virt_lines_above,
			"fallback occupies a file-header card"
		)
		local card = vim.inspect(marks[1][4].virt_lines)
		assert(
			card:find("Outdated", 1, true) and card:find("Original lines", 1, true) and card:find("selected", 1, true)
		)
		api.nvim_set_current_win(state.panes.new.win)
		api.nvim_win_set_cursor(0, { 1, 0 })
		H.keys("<CR>")
		local popup = require("lgtm.threads").last
		local text = table.concat(api.nvim_buf_get_lines(popup.buf, 0, -1, false), "\n")
		assert(
			text:find("OUTDATED", 1, true) and text:find("Original lines 2", 1, true) and text:find("selected", 1, true)
		)
		local bounds = api.nvim_win_get_config(popup.win)
		assert(bounds.row == math.floor((vim.o.lines - bounds.height - 2) / 2))
		H.keys("q")
	end)
	H.scenario("Retain deleted-file discussions as orphaned items and reconnect matching restored text", function()
		git(root, "rm", "--", moved)
		git(root, "commit", "-qm", "delete reviewed file")
		state = open(H, "branch", path, true)
		H.wait(function()
			return thread(state, root_id).orphanReason == "file_deleted"
		end, "record confirmed file deletion")
		vim.cmd("Lgtm threads")
		local text = table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(
			text:find("ORPHANED", 1, true)
				and text:find("File was deleted", 1, true)
				and text:find("Preserve this staged selection", 1, true)
		)
		H.keys("q")
		vim.fn.mkdir(root .. "/moved", "p")
		bytes(root .. "/" .. moved, relocated_text)
		git(root, "add", "--", moved)
		git(root, "commit", "-qm", "restore matching source")
		state = open(H, "branch", moved, true)
		H.wait(function()
			return (thread(state, root_id).orphanReason or "") == ""
		end, "reconnect restored evidence")
		assert(placement(state, root_id).location.start == 3)
		assert(vim.deep_equal(thread(state, root_id).anchor, original))
	end)
	H.scenario("Hide discussions outside the staged diff without orphaning them or losing exports", function()
		bytes(root .. "/other.txt", "working selection\n")
		git(root, "add", "other.txt")
		state = open(H, "staged", "other.txt", true)
		assert(#state.conversation.comments == 2 and (thread(state, root_id).orphanReason or "") == "")
		assert(#api.nvim_buf_get_extmarks(state.panes.new.buf, require("lgtm.threads").namespace, 0, -1, {}) == 0)
		vim.cmd("Lgtm threads")
		local text = table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(text:find("Preserve this staged selection", 1, true) and not text:find("ORPHANED", 1, true))
		H.keys("q")
		vim.cmd("Lgtm complete")
		H.wait(function()
			return state.review.state == "completed"
		end, "complete staged view without changing hidden comments")
		git(root, "reset", "--mixed", "HEAD")
	end)
	H.scenario("Orphan changed unstaged anchors while preserving feedback and readable exports", function()
		vim.cmd.edit(vim.fn.fnameescape(root .. "/other.txt"))
		local before_mapping = vim.fn.maparg("c", "n", false, true)
		state = open(H, "working", "other.txt")
		api.nvim_set_current_win(state.panes.new.win)
		api.nvim_win_set_cursor(0, { 1, 0 })
		H.keys("c")
		H.compose("Retain this unstaged feedback")
		for _, value in ipairs(state.conversation.threads) do
			if value.anchor.path == "other.txt" then
				working_id = value.id
				assert(value.anchor.source.workingFile)
			end
		end
		assert(working_id)
		bytes(root .. "/other.txt", "changed working selection\n")
		vim.cmd("Lgtm refresh")
		H.wait(function()
			return state.render_ready and thread(state, working_id).orphanReason == "working_source_changed"
		end, "persist unstaged-source orphaning")
		assert(placement(state, working_id).status == "orphaned")
		local before = H.rpc("GetConversation", { scope = state.scope })
		H.choice = "Preview"
		vim.cmd("Lgtm export review")
		H.wait(function()
			return vim.bo.filetype == "lgtm"
				and table
					.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
					:find("Retain this unstaged feedback", 1, true)
		end, "readable export preview")
		local markdown = table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(markdown:find("1. **[COMMENT]**", 1, true) and markdown:find("Keep this reply", 1, true))
		assert(not markdown:find(root_id, 1, true) and not markdown:find(oid, 1, true))
		H.keys("q")
		assert(
			vim.deep_equal(H.rpc("GetConversation", { scope = state.scope }), before),
			"editor export never mutates review state"
		)
		vim.fn.writefile({
			vim.json.encode({
				root = root,
				reviewId = state.review.id,
				threadId = root_id,
				replyId = reply_id,
				workingId = working_id,
				original = original,
			}),
		}, vim.env.LGTM_HOME .. "/anchor-proof.json")
		local working_buf = state.panes.new.buf
		vim.cmd("Lgtm close")
		local after_mapping = api.nvim_buf_call(working_buf, function()
			return vim.fn.maparg("c", "n", false, true)
		end)
		assert(
			after_mapping.callback == before_mapping.callback and after_mapping.rhs == before_mapping.rhs,
			"closing a review restores the original normal-file/Kak comment key"
		)
	end)
	vim.cmd.cd(vim.env.LGTM_E2E_WORKSPACE)
end
