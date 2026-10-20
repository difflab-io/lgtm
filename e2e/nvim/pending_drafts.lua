-- fixtures --------------------------------------------------------------------
local function status(mode)
	local args = { "curl", "-sS", vim.env.LGTM_E2E_GITHUB .. (mode and "/__control" or "/__status") }
	if mode then
		vim.list_extend(args, { "-H", "Content-Type: application/json", "-d", vim.json.encode({ mode = mode }) })
	end
	return vim.json.decode(vim.system(args, { text = true }):wait().stdout)
end

local function open(H, number)
	local review = H.rpc("StartReview", {
		target = { pullRequest = { repositoryUrl = "https://github.com/acme/repo", number = number } },
		requestKey = require("lgtm.context").request_key(),
	}).review
	local state = require("lgtm.context").apply({}, review)
	require("lgtm.review").open(state)
	H.wait(function()
		return state.render_ready
	end, "PR diff and existing conversation")
	vim.api.nvim_set_current_win(state.panes.new.win)
	vim.api.nvim_win_set_cursor(0, { 2, 0 })
	return state
end

local function popup_comment(H, body)
	H.keys("<CR>")
	local popup = require("lgtm.threads").last
	for line, row in ipairs(popup.rows) do
		if row.comment and row.comment.body == body then
			vim.api.nvim_win_set_cursor(popup.win, { line, 0 })
			return
		end
	end
	error("Comment absent from popup: " .. body)
end

-- tests -----------------------------------------------------------------------
return function(H)
	H.scenario("Save, edit, reply to and delete GitHub pending drafts through the comment modal", function()
		local state = open(H, 4)
		H.keys("c")
		H.compose("Pending draft before editor restart")
		H.wait(function()
			return #state.conversation.comments == 1
		end, "saved draft overlay")
		local saved = state.conversation.comments[1]
		assert(saved.visibility == "draft" and saved.postingStatus == "confirmed" and saved.forgeCommentId ~= "")
		local remote = status()
		local pending
		for _, review in ipairs(remote.reviews) do
			if review.id == 14 then
				pending = review
			end
		end
		assert(pending and pending.state == "PENDING", "save creates a private pending review, never submits")
		assert(
			#remote.threads["4"] == 1 and (not remote.comments["4"] or #remote.comments["4"] == 0),
			"inline save must not publish issue comments"
		)
		popup_comment(H, saved.body)
		H.keys("e")
		H.compose("Edited pending draft")
		H.wait(function()
			return state.conversation.comments[1].body == "Edited pending draft"
		end, "edit pending comment")
		popup_comment(H, "Edited pending draft")
		H.keys("c")
		H.compose("Pending reply to remove")
		H.wait(function()
			return #state.conversation.comments == 2
		end, "reply in same GitHub pending review")
		popup_comment(H, "Pending reply to remove")
		H.choice = "Delete"
		H.keys("dd")
		H.wait(function()
			local visible = 0
			for _, c in ipairs(state.conversation.comments) do
				if c.remotePresence ~= "missing" then
					visible = visible + 1
				end
			end
			return visible == 1
		end, "deleted reply disappears from conversation")
		assert(#status().threads["4"][1].comments.nodes == 1, "deletion reaches GitHub")
		vim.cmd("Lgtm close")
	end)
	H.scenario("Recover an accepted draft after a lost response without reposting it", function()
		local state = open(H, 5)
		status("draft_lost")
		H.keys("c")
		H.compose("Draft whose response was lost")
		H.wait(function()
			return #state.conversation.comments == 1
		end, "durable local draft after response loss")
		assert(state.conversation.comments[1].postingStatus == "uncertain")
		local writes = status("normal").writes
		vim.cmd("Lgtm sync")
		H.wait(function()
			return state.conversation.comments[1].postingStatus == "confirmed"
		end, "recover exact remote draft receipt")
		assert(status().writes == writes, "recovery is read-only and cannot duplicate the comment")
		status("denied")
		H.keys("c")
		H.compose("Draft retained after permission failure")
		H.wait(function()
			return #state.conversation.comments == 2
		end, "permission failure preserves local draft")
		status("normal")
		vim.cmd("Lgtm sync")
		H.wait(function()
			for _, c in ipairs(state.conversation.comments) do
				if c.postingStatus ~= "confirmed" then
					return false
				end
			end
			return true
		end, "explicitly retry a rejected save after permission recovery")
		assert(#status().threads["5"] == 2)
		vim.cmd("Lgtm close")
	end)
	H.scenario("Recover pending review creation after a lost response without creating another review", function()
		local state = open(H, 6)
		status("review_lost")
		H.keys("c")
		H.compose("Draft after pending review creation response was lost")
		H.wait(function()
			return #state.conversation.comments == 1
		end, "save retained while pending creation is uncertain")
		local writes = status("normal").writes
		vim.cmd("Lgtm sync")
		H.wait(function()
			return state.conversation.comments[1].postingStatus == "confirmed"
		end, "recover existing pending review")
		assert(status().writes == writes + 1, "only the missing comment is added; pending review is reused")
		vim.cmd("Lgtm close")
	end)
	H.scenario("Retain and label saved feedback after its private GitHub review is discarded externally", function()
		local state = open(H, 7)
		H.keys("c")
		H.compose("Saved feedback after external discard")
		H.wait(function()
			return #state.conversation.comments == 1 and state.conversation.comments[1].postingStatus == "confirmed"
		end, "private draft saved")
		local response = vim.system({
			"curl",
			"-sS",
			vim.env.LGTM_E2E_GITHUB .. "/__control",
			"-H",
			"Content-Type: application/json",
			"-d",
			vim.json.encode({ discard_pending = true, discard_pr = 7 }),
		}, { text = true }):wait()
		assert(response.code == 0)
		local writes = vim.json.decode(response.stdout).writes
		vim.cmd("Lgtm refresh")
		H.wait(function()
			return state.conversation.comments[1].remotePresence == "missing"
		end, "complete observation confirms external discard")
		local marks = vim.api.nvim_buf_get_extmarks(
			state.panes.new.buf,
			require("lgtm.threads").namespace,
			0,
			-1,
			{ details = true }
		)
		local text = vim.inspect(marks)
		assert(
			text:find("removed on GitHub", 1, true) and not text:find("GitHub draft", 1, true),
			"card does not claim a discarded private draft still exists remotely"
		)
		local before = H.rpc("GetConversation", { scope = state.scope })
		H.choice = "Preview"
		vim.cmd("Lgtm export review")
		H.wait(function()
			return vim.api.nvim_buf_get_lines(0, 0, 1, false)[1] == "# Review feedback"
		end, "discarded-draft feedback preview")
		local markdown = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(
			markdown:find("Saved feedback after external discard", 1, true)
				and markdown:find("no longer on GitHub", 1, true),
			"export retains local feedback and explains missing private draft"
		)
		H.keys("q")
		assert(vim.deep_equal(before, H.rpc("GetConversation", { scope = state.scope })))
		assert(status().writes == writes, "observation and export never recreate a discarded remote review")
		vim.cmd("Lgtm close")
	end)
end
