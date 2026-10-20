-- tests -----------------------------------------------------------------------
return function(H)
	H.scenario("Restart the backend and editor and resume moved notes and ignored-directory feedback", function()
		local proof =
			vim.json.decode(table.concat(vim.fn.readfile(vim.env.LGTM_HOME .. "/notes-move-proof.json"), "\n"))
		vim.cmd.edit(vim.fn.fnameescape(proof.notePath))
		vim.cmd("Lgtm notes")
		H.wait(function()
			return require("lgtm.threads").last and vim.api.nvim_win_is_valid(require("lgtm.threads").last.win)
		end, "moved notes thread list")
		local notes = H.rpc("GetConversation", { scope = { notes = { path = proof.notePath } } })
		assert(#notes.comments == 2)
		assert(vim.iter(notes.comments):any(function(comment)
			return comment.id == proof.noteId
		end))
		H.keys("q")
		vim.cmd("Lgtm directory " .. proof.root)
		H.wait(function()
			local state = require("lgtm.context").current
			return state and state.render_ready and state.conversation
		end, "resume moved ignored directory")
		local review = require("lgtm.context").current
		assert(review.review.id == proof.reviewId and review.root == proof.root)
		assert(#review.conversation.comments == 2)
		assert(vim.iter(review.conversation.comments):any(function(comment)
			return comment.id == proof.commentId
		end))
		vim.cmd("Lgtm close")
	end)
end
