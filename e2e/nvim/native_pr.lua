-- tests -----------------------------------------------------------------------
local function should_use_native_git_explorer_without_remote_contents(H)
	local api = vim.api
	local root = vim.fn.tempname()
	vim.fn.mkdir(root .. "/src", "p")
	root = vim.uv.fs_realpath(root)
	local function git(...)
		local args = { "git", "-C", root, "-c", "user.name=E2E", "-c", "user.email=e2e@example.test" }
		vim.list_extend(args, { ... })
		local result = vim.system(args, { text = true }):wait()
		assert(result.code == 0, result.stderr)
		return vim.trim(result.stdout)
	end
	git("init", "-q", "-b", "main")
	git("remote", "add", "origin", "git@github.com:acme/repo.git")
	vim.fn.writefile({ "old" }, root .. "/review.txt")
	vim.fn.writefile({ "pub fn old() {}", "// second line", "// third line" }, root .. "/src/lib.rs")
	git("add", ".")
	git("commit", "-qm", "base")
	local base = git("rev-parse", "HEAD")
	git("checkout", "-qb", "feature")
	vim.fn.writefile({ "new" }, root .. "/review.txt")
	vim.fn.writefile(
		{ "pub fn new() {}", "// second line", "// inserted context", "// third line" },
		root .. "/src/lib.rs"
	)
	git("add", ".")
	git("commit", "-qm", "head")
	local head = git("rev-parse", "HEAD")
	local function http(path, data)
		local args = { "curl", "-sS", vim.env.LGTM_E2E_GITHUB .. path }
		if data then
			vim.list_extend(args, { "-H", "Content-Type: application/json", "-d", vim.json.encode(data) })
		end
		return vim.json.decode(vim.system(args, { text = true }):wait().stdout)
	end
	local state, reads
	local function assert_seen(path, seen)
		assert(not not (state.reviewed or {})[path] == seen, "reviewed state for " .. path)
		local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
		local visible = false
		for _, mark in
			ipairs(
				api.nvim_buf_get_extmarks(
					panel.bufnr,
					api.nvim_create_namespace("lgtm.reviewed"),
					0,
					-1,
					{ details = true }
				)
			)
		do
			local node = panel.tree:get_node(mark[2] + 1)
			if node and node.data.path == path then
				visible = mark[4].virt_text[1][1]:find("✓", 1, true) ~= nil
			end
		end
		assert(visible == seen, "visible checkmark for " .. path)
	end
	local function toggle_current_file()
		local path = state.file.path
		api.nvim_set_current_win(state.panes.new.win)
		H.keys(" rm")
		assert_seen(path, true)
		H.keys(" rm")
		assert_seen(path, false)
	end
	H.scenario(
		"Infer the branch PR, navigate the picker, and open the native tree before delayed comment loading",
		function()
			http("/__control", { mode = "normal", base = base, head = head, comments_delay = 2 })
			reads = http("/__status").content_reads
			vim.cmd.cd(root)
			H.manual_picker = true
			vim.cmd("Lgtm prs")
			H.wait(function()
				local p = require("lgtm.picker").last
				return p and not p.closed
			end, "native PR picker")
			local p = require("lgtm.picker").last
			H.keys("<Down>")
			assert(p.index == 2, "Down selects next PR")
			H.keys("<Up>")
			assert(p.index == 1, "Up selects previous PR")
			assert(
				#api.nvim_buf_get_extmarks(
						p.list_buf,
						api.nvim_create_namespace("lgtm.picker.selection"),
						0,
						-1,
						{}
					) == 1,
				"selection remains highlighted in inactive result window"
			)
			H.keys("<CR>")
			H.manual_picker = false
			H.wait(function()
				local s = require("lgtm.context").current
				return s and s.native_explorer and s.render_ready
			end, "native Git PR diff")
			state = require("lgtm.context").current
			assert(state.git_root == root and not state.temp_dir, "native Git mode must not materialize snapshots")
			assert(#state.files == 2)
			assert(state.initializing, "native tree must open before delayed GitHub comment refresh completes")
			local session = require("codediff.ui.lifecycle").get_session(state.tab)
			assert(session.panel.name == "explorer" and session.panel.view.tree, "real CodeDiff tree")
			for _, pane in pairs(state.panes) do
				assert(
					api.nvim_buf_get_name(pane.buf):match("^codediff://"),
					"revision uses CodeDiff Git virtual buffer"
				)
				assert(vim.bo[pane.buf].buftype == "nowrite", "revision cannot attach normal-file language servers")
			end
			assert(http("/__status").content_reads == reads, "local PR review never downloads GitHub file contents")
		end
	)
	H.scenario("Mark and unmark PR files with checkmarks, targeting the selected tree file", function()
		toggle_current_file()
		local previous = state.file.path
		local selected = previous == "review.txt" and "src/lib.rs" or "review.txt"
		local panel = require("codediff.ui.lifecycle").get_panel_view(state.tab)
		api.nvim_set_current_win(panel.winid)
		local row
		for line = 1, api.nvim_buf_line_count(panel.bufnr) do
			local node = panel.tree:get_node(line)
			if node and node.data.path == selected then
				row = line
				break
			end
		end
		assert(row, "other file visible in native tree")
		api.nvim_win_set_cursor(panel.winid, { row, 0 })
		assert(state.file.path == previous, "moving the tree cursor does not change the loaded file")
		H.keys("m")
		H.wait(function()
			return state.file.path == selected and state.render_ready and (state.reviewed or {})[selected]
		end, "selected tree file loaded and marked with its actual content identity")
		assert_seen(selected, true)
		assert_seen(previous, false)
		panel.tree:render()
		H.wait(function()
			return #api.nvim_buf_get_extmarks(panel.bufnr, api.nvim_create_namespace("lgtm.reviewed"), 0, -1, {}) == 1
		end, "checkmark survives tree redraw")
		api.nvim_set_current_win(panel.winid)
		api.nvim_win_set_cursor(panel.winid, { row, 0 })
		H.keys("m")
		assert_seen(selected, false)
	end)
	H.scenario("Navigate the native tree and create a collapsed comment that opens in a centered modal", function()
		state.show_file("src/lib.rs")
		H.wait(function()
			return state.render_ready and state.file.path == "src/lib.rs"
		end, "native tree file navigation")
		assert(api.nvim_buf_get_lines(state.panes.new.buf, 0, 1, false)[1] == "pub fn new() {}")
		H.wait(function()
			return not state.initializing
		end, "background PR comment sync")
		api.nvim_set_current_win(state.panes.new.win)
		H.keys("c")
		H.compose(
			"Native CodeDiff comment with optional Kak\nSecond line is shown only after expansion\nThird detail line"
		)
		H.wait(function()
			return #state.conversation.comments > 0
		end, "c persists native revision comment")
		assert(
			api.nvim_buf_get_lines(state.panes.new.buf, 0, 1, false)[1] == "pub fn new() {}",
			"c must not edit read-only revision content"
		)
		local marks =
			api.nvim_buf_get_extmarks(state.panes.new.buf, require("lgtm.threads").namespace, 0, -1, { details = true })
		assert(#marks > 0)
		for _, mark in ipairs(marks) do
			local details = mark[4]
			assert(not details.virt_text, "comments occupy their own card rows, never end-of-line text")
			assert(
				details.virt_lines_leftcol and #details.virt_lines == 3,
				"collapsed card spans the full pane in three rows"
			)
			local card_text = vim.inspect(details.virt_lines)
			assert(
				card_text:find("GitHub draft", 1, true) and card_text:find("Native CodeDiff", 1, true),
				"card shows status and first-line preview: " .. card_text
			)
			assert(not card_text:find("Second line", 1, true), "collapsed preview stays one line")
			for _, row in ipairs(details.virt_lines) do
				local text = ""
				for _, chunk in ipairs(row) do
					text = text .. chunk[1]
				end
				assert(
					vim.fn.strdisplaywidth(text) == api.nvim_win_get_width(state.panes.new.win),
					"card fills its pane without overflow"
				)
			end
		end
		local left_row = vim.fn.screenpos(state.panes.old.win, 3, 1).row
		local right_row = vim.fn.screenpos(state.panes.new.win, 4, 1).row
		assert(
			left_row == right_row and left_row > 0,
			"comment cards preserve unchanged-line alignment across unequal diff sides"
		)
		H.keys("<CR>")
		H.wait(function()
			return require("lgtm.threads").last and api.nvim_get_current_win() == require("lgtm.threads").last.win
		end, "draft comment popup")
		assert(table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Native CodeDiff comment", 1, true))
		H.keys("q")
		-- Kak extends selections with uppercase movement keys.
		H.keys(vim.env.LGTM_E2E_KAK == "1" and "VJ" or "Vj")
		H.keys("c")
		H.compose("Range feedback from the visual comment shortcut")
		local range
		for _, thread in ipairs(state.conversation.threads) do
			local anchor = thread.anchor
			if anchor.path == "src/lib.rs" and tonumber(anchor.start) == 1 and tonumber(anchor["end"]) == 2 then
				range = anchor
			end
		end
		assert(
			range
				and range.side == "new"
				and range.source.commit.sha == head
				and range.evidence.selectedText == "pub fn new() {}\n// second line",
			"visual c preserves selected lines and diff side"
		)
		assert(http("/__status").content_reads == reads)
	end)
	H.scenario(
		"Keep provider-outdated PR comments above the file even when their original text still exists",
		function()
			local original = vim.deepcopy(state.conversation.threads[1].anchor)
			http("/__control", { outdated_pr = 1, outdated = true, comments_delay = 0 })
			vim.cmd("Lgtm refresh")
			H.wait(function()
				local thread = state.conversation.threads[1]
				return thread.forgeObservation and thread.forgeObservation.outdated and state.render_ready
			end, "provider outdated observation")
			assert(
				vim.deep_equal(original, state.conversation.threads[1].anchor),
				"provider refresh preserves immutable source evidence"
			)
			local marks = api.nvim_buf_get_extmarks(
				state.panes.new.buf,
				require("lgtm.threads").namespace,
				0,
				-1,
				{ details = true }
			)
			assert(#marks > 0, "outdated discussions remain visible")
			for _, mark in ipairs(marks) do
				assert(mark[2] == 0 and mark[4].virt_lines_above, "outdated cards float above the first code line")
				assert(
					vim.inspect(mark[4].virt_lines):find("Outdated", 1, true),
					"card clearly labels the provider state"
				)
			end
			api.nvim_set_current_win(state.panes.new.win)
			api.nvim_win_set_cursor(0, { 1, 0 })
			H.keys("<CR>")
			local text = table.concat(api.nvim_buf_get_lines(0, 0, -1, false), "\n")
			assert(
				text:find("OUTDATED", 1, true) and text:find("Original lines", 1, true),
				"thread modal shows lost original location"
			)
			H.keys("q")
			http("/__control", { outdated_pr = 1, outdated = false })
			vim.cmd("Lgtm refresh")
			H.wait(function()
				return not state.conversation.threads[1].forgeObservation.outdated and state.render_ready
			end, "restored current provider location")
			assert(vim.deep_equal(original, state.conversation.threads[1].anchor))
			assert(http("/__status").content_reads == reads, "provider status refresh does not download local source")
			vim.cmd("Lgtm close")
		end
	)
	H.scenario("Review and complete a local branch from its merge base without GitHub", function()
		-- A branch review needs no forge and uses the common ancestor of the chosen base.
		H.choice = "branch"
		vim.cmd("Lgtm review")
		H.wait(function()
			local s = require("lgtm.context").current
			return s and s.native_explorer and s.render_ready
		end, "local branch review")
		state = require("lgtm.context").current
		assert(state.kind == "git" and state.base == base and state.head == head)
		assert(require("codediff.ui.lifecycle").get_panel_name(state.tab) == "explorer")
		toggle_current_file()
		vim.cmd("Lgtm complete")
		H.wait(function()
			return state.review.state == "completed"
		end, "local branch completion")
		vim.cmd("Lgtm close")
	end)
	H.scenario("Show clear empty states for clean working and staged comparisons", function()
		for _, mode in ipairs({ "working", "staged" }) do
			local before = #require("lgtm.notifications").history
			H.choice = mode
			vim.cmd("Lgtm review")
			H.wait(function()
				for i = before + 1, #require("lgtm.notifications").history do
					if
						require("lgtm.notifications").history[i].message
						== (mode == "working" and "No working-tree changes" or "No staged changes")
					then
						return true
					end
				end
			end, "clear empty " .. mode .. " comparison")
			local empty = require("lgtm.context").current
			vim.cmd("Lgtm complete")
			H.wait(function()
				return empty.review.state == "completed"
			end, "complete empty comparison")
			vim.cmd("Lgtm close")
		end
	end)
	H.scenario("Review saved working changes in the native CodeDiff explorer", function()
		vim.fn.writefile({ "unstaged" }, root .. "/review.txt")
		H.choice = "working"
		vim.cmd("Lgtm review")
		H.wait(function()
			local s = require("lgtm.context").current
			return s and s.render_ready
		end, "native working changes")
		state = require("lgtm.context").current
		assert(require("codediff.ui.lifecycle").get_panel_name(state.tab) == "explorer")
		assert(api.nvim_buf_get_lines(state.panes.new.buf, 0, 1, false)[1] == "unstaged")
		toggle_current_file()
		H.keys(" rm")
		vim.fn.writefile({ "working content changed after marking seen" }, root .. "/review.txt")
		vim.cmd("Lgtm refresh")
		H.wait(function()
			return state.render_ready and state.reviewed["review.txt"] == nil
		end, "working content change clears reviewed checkmark")
		assert_seen("review.txt", false)
		vim.cmd("Lgtm close")
	end)
	H.scenario("Restore ordinary-file mappings while a shared working buffer is outside its review tab", function()
		vim.cmd.edit(vim.fn.fnameescape(root .. "/review.txt"))
		api.nvim_win_set_cursor(0, { 1, 0 })
		vim.cmd("Lgtm note")
		H.compose("Standalone note outside the review")
		local before = vim.fn.maparg("c", "n", false, true)
		H.choice = "working"
		vim.cmd("Lgtm review")
		H.wait(function()
			local current = require("lgtm.context").current
			return current and current.render_ready
		end, "reopen saved working review")
		state = require("lgtm.context").current
		local review_tab, working_buf = state.tab, state.panes.new.buf
		vim.cmd("tabnew " .. vim.fn.fnameescape(root .. "/review.txt"))
		assert(api.nvim_get_current_buf() == working_buf, "ordinary tab shares the actual working buffer")
		local outside = vim.fn.maparg("c", "n", false, true)
		assert(
			outside.callback == before.callback and outside.rhs == before.rhs,
			"normal or Kak c mapping restored on tab leave"
		)
		assert(not vim.b[working_buf].lgtm, "ordinary tab is not tagged as an LGTM review")
		H.wait(function()
			return vim.inspect(
				api.nvim_buf_get_extmarks(working_buf, require("lgtm.threads").namespace, 0, -1, { details = true })
			):find("Standalone note outside", 1, true)
		end, "ordinary tab refreshes and displays its own notes")
		vim.api.nvim_exec_autocmds("ColorScheme", { modeline = false })
		vim.cmd("tabclose")
		H.wait(function()
			return api.nvim_get_current_tabpage() == review_tab
				and state.render_ready
				and api.nvim_buf_call(working_buf, function()
					return vim.fn.maparg("c", "n", false, true).desc == "Add review comment"
				end)
		end, "review tab restores its comment mapping and source placement")
		assert(
			not vim.inspect(
				api.nvim_buf_get_extmarks(working_buf, require("lgtm.threads").namespace, 0, -1, { details = true })
			)
				:find("Standalone note outside", 1, true),
			"ordinary notes never replace review overlays"
		)
	end)
	H.scenario(
		"Refresh native file metadata and invalidate changed hidden files without rereading their bodies",
		function()
			vim.fn.writefile({ "pub fn hidden_change() {}" }, root .. "/src/lib.rs")
			vim.cmd("Lgtm refresh")
			H.wait(function()
				return state.render_ready and #state.files == 2
			end, "newly changed file joins the native review metadata")
			state.show_file("src/lib.rs")
			H.wait(function()
				return state.render_ready and state.file.path == "src/lib.rs"
			end, "select file added by native refresh")
			api.nvim_set_current_win(state.panes.new.win)
			H.keys(" rm")
			assert_seen("src/lib.rs", true)
			state.show_file("review.txt")
			H.wait(function()
				return state.render_ready and state.file.path == "review.txt"
			end, "leave the marked file hidden")
			api.nvim_set_current_win(state.panes.new.win)
			H.keys(" rm")
			assert_seen("review.txt", true)
			vim.fn.writefile({ "pub fn changed_after_seen() {}" }, root .. "/src/lib.rs")
			vim.cmd("Lgtm refresh")
			H.wait(function()
				return state.render_ready and not state.reviewed["src/lib.rs"]
			end, "hidden working-source change invalidates its checkmark")
			assert_seen("src/lib.rs", false)
			assert_seen("review.txt", true)
			assert(http("/__status").content_reads == reads, "local refresh never downloads provider content")
			vim.cmd("Lgtm close")
		end
	)
	H.scenario("Review staged changes from the native Git index buffer", function()
		git("add", "review.txt", "src/lib.rs")
		H.choices = { "staged", "Complete saved review and start this comparison" }
		vim.cmd("Lgtm review")
		H.wait(function()
			local s = require("lgtm.context").current
			return s and s.render_ready
		end, "native staged changes")
		state = require("lgtm.context").current
		assert(api.nvim_buf_get_name(state.panes.new.buf):match("^codediff://"), "index shown in native virtual buffer")
		state.show_file("review.txt")
		H.wait(function()
			return state.render_ready and state.file.path == "review.txt"
		end, "select staged review file in the native tree")
		assert(
			api.nvim_buf_get_lines(state.panes.new.buf, 0, 1, false)[1] == "working content changed after marking seen"
		)
		toggle_current_file()
		api.nvim_set_current_win(state.panes.new.win)
		H.keys(" rm")
		assert_seen("review.txt", true)
		state.show_file("src/lib.rs")
		H.wait(function()
			return state.render_ready and state.file.path == "src/lib.rs"
		end, "select other staged file")
		api.nvim_set_current_win(state.panes.new.win)
		H.keys(" rm")
		assert_seen("src/lib.rs", true)
		vim.fn.writefile({ "staged again after marking seen" }, root .. "/review.txt")
		git("add", "review.txt")
		vim.cmd("Lgtm refresh")
		H.wait(function()
			return state.render_ready and not state.reviewed["review.txt"]
		end, "hidden index blob change clears its checkmark")
		assert_seen("review.txt", false)
		assert_seen("src/lib.rs", true)
		vim.cmd("Lgtm close")
	end)
	H.scenario("Resume saved commits or explicitly complete them before changing the branch comparison", function()
		local saved = vim.deepcopy(state.review)
		git("commit", "-qm", "another change")
		local next_head = git("rev-parse", "HEAD")
		H.choices = { "branch", "Resume saved review" }
		vim.cmd("Lgtm review")
		H.wait(function()
			local s = require("lgtm.context").current
			return s and s.render_ready
		end, "resume comparison pinned to saved commits")
		state = require("lgtm.context").current
		assert(
			state.review.id == saved.id and vim.deep_equal(state.comparison, saved.comparison),
			"resume preserves reviewed commits"
		)
		vim.cmd("Lgtm close")
		H.choices = { "branch", "Complete saved review and start this comparison" }
		vim.cmd("Lgtm review")
		H.wait(function()
			local s = require("lgtm.context").current
			return s and s.render_ready
		end, "start explicitly selected branch comparison")
		state = require("lgtm.context").current
		assert(state.review.id ~= saved.id and state.head == next_head and state.base == base)
		local entries = H.rpc("ListReviews", { target = { localGit = { path = root } }, state = "completed" }).reviews
		local completed
		for _, entry in ipairs(entries) do
			if entry.id == saved.id then
				completed = entry.state == "completed"
			end
		end
		assert(completed, "changing comparisons retains the completed saved review")
		vim.cmd("Lgtm close")
	end)
	H.scenario("Show a clean native comparison after the last working change is restored", function()
		H.choice = "branch"
		vim.cmd("Lgtm review")
		H.wait(function()
			local current = require("lgtm.context").current
			return current and current.render_ready
		end, "resume branch before completing its review")
		state = require("lgtm.context").current
		vim.cmd("Lgtm complete")
		H.wait(function()
			return state.review.state == "completed"
		end, "complete branch review")
		vim.cmd("Lgtm close")
		vim.fn.writefile({ "temporary working edit" }, root .. "/review.txt")
		H.choice = "working"
		vim.cmd("Lgtm review")
		H.wait(function()
			local current = require("lgtm.context").current
			return current and current.render_ready
		end, "review temporary working change")
		state = require("lgtm.context").current
		git("restore", "review.txt")
		vim.cmd("Lgtm refresh")
		H.wait(function()
			return state.empty and #state.files == 0 and state.file == nil
		end, "native welcome after the last change disappears")
		assert(
			vim.tbl_isempty(state.views) and not state.observe_sources,
			"empty comparison retains no stale source placement or mutation callback"
		)
		assert(require("codediff.ui.lifecycle").get_session(state.tab), "the native review remains open")
		vim.cmd("Lgtm close")
	end)
	vim.cmd.cd(vim.env.LGTM_E2E_WORKSPACE)
	vim.fn.delete(root, "rf")
end
return should_use_native_git_explorer_without_remote_contents
