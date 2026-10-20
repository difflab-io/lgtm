-- tests -----------------------------------------------------------------------
return function(H)
	H.scenario("Restart the editor and backend with preserved moved discussions and unstaged orphans", function()
		local proof = vim.json.decode(table.concat(vim.fn.readfile(vim.env.LGTM_HOME .. "/anchor-proof.json"), "\n"))
		vim.cmd.cd(proof.root)
		local review = H.rpc("GetReview", { reviewId = proof.reviewId }).review
		local state = require("lgtm.context").apply({}, review)
		require("lgtm.review").open(state)
		H.wait(function()
			return state.render_ready and state.conversation
		end, "resume stored review and actual selected sources")
		local moved, orphan
		for _, thread in ipairs(state.conversation.threads) do
			if thread.id == proof.threadId then
				moved = thread
			end
			if thread.id == proof.workingId then
				orphan = thread
			end
		end
		assert(moved and moved.currentPath == "moved/source [literal] ü.txt" and (moved.orphanReason or "") == "")
		assert(vim.deep_equal(moved.anchor, proof.original), "restart retains original staged evidence")
		assert(
			orphan and orphan.orphanReason == "working_source_changed",
			"restart cannot silently reattach changed unstaged text"
		)
		assert(
			vim.iter(state.conversation.comments):any(function(comment)
				return comment.id == proof.replyId
			end),
			"reply identity survives every source change and restart"
		)
		vim.cmd("Lgtm threads")
		local text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
		assert(
			text:find("Preserve this staged selection", 1, true)
				and text:find("Retain this unstaged feedback", 1, true)
				and text:find("ORPHANED", 1, true)
		)
		H.keys("q")
		vim.cmd("Lgtm close")
		vim.cmd.cd(vim.env.LGTM_E2E_WORKSPACE)
	end)
end
