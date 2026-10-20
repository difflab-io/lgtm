-- tests -----------------------------------------------------------------------
return function(H)
	H.scenario(
		"Restart editor and backend, resume pending drafts, then submit the same review with a final comment",
		function()
			local selected
			for _, entry in ipairs(H.rpc("ListReviews", {}).reviews) do
				if tonumber(entry.target.pullRequest and entry.target.pullRequest.number) == 4 then
					selected = entry
				end
			end
			assert(selected and selected.forgeReviewId == "14", "pending review receipt survives restart")
			local refreshed = H.rpc("RefreshReview", {
				scope = { reviewId = selected.id },
				expectedVersion = selected.version,
				requestKey = require("lgtm.context").request_key(),
			}).review
			local state = require("lgtm.context").apply({}, refreshed)
			require("lgtm.review").open(state)
			H.wait(function()
				return state.render_ready
			end, "resumed review diff")
			local visible = {}
			for _, c in ipairs(state.conversation.comments) do
				if c.remotePresence ~= "missing" then
					visible[#visible + 1] = c
				end
			end
			assert(#visible == 1 and visible[1].body == "Edited pending draft" and visible[1].visibility == "draft")
			vim.api.nvim_set_current_win(state.panes.new.win)
			H.choice = "comment"
			H.keys(" rs")
			H.compose("Final body after restart")
			assert(state.review.forgeReviewId == "14" and state.review.submissionStatus == "confirmed")
			local remote = vim.json.decode(
				vim.system({ "curl", "-sS", vim.env.LGTM_E2E_GITHUB .. "/__status" }, { text = true }):wait().stdout
			)
			local review
			for _, value in ipairs(remote.reviews) do
				if value.id == 14 then
					review = value
				end
			end
			assert(review and review.state == "COMMENTED" and review.body:find("Final body after restart", 1, true))
			assert(#remote.threads["4"] == 1, "submit does not repost the saved draft")
			local before_export = H.rpc("GetConversation", { scope = state.scope })
			H.choice = "Preview"
			vim.cmd("Lgtm export review")
			H.wait(function()
				return vim.api.nvim_buf_get_lines(0, 0, 1, false)[1] == "# Review feedback"
			end, "submitted review Markdown preview")
			local markdown = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
			assert(
				markdown:find("## Overall feedback", 1, true) and markdown:find("Final body after restart", 1, true),
				"editor exports the submitted final comment"
			)
			assert(
				markdown:find("Edited pending draft", 1, true),
				"saved selected feedback is retained alongside the final body"
			)
			assert(not markdown:find(selected.id, 1, true), "editor export omits review storage IDs")
			H.keys("q")
			assert(
				vim.deep_equal(before_export, H.rpc("GetConversation", { scope = state.scope })),
				"preview does not mutate submitted feedback"
			)
			vim.cmd("Lgtm close")
		end
	)
end
