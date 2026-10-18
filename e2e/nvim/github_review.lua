-- tests -----------------------------------------------------------------------
local function should_support_github_review(H)
	local api = vim.api
	local rpc = require("lgtm.rpc")
	local latest = rpc.latest
	local branch = vim.trim(vim.system({ "git", "branch", "--show-current" }, { text = true }):wait().stdout)
	rpc.latest = function(key, method, input, callback)
		if method == "ListPullRequests" then
			assert(input.repositoryUrl == "https://github.com/acme/repo", "infer SSH origin repository")
			assert(input.query == "head:" .. branch, "limit PRs to the current branch")
		end
		return latest(key, method, input, callback)
	end
	local select = vim.ui.select
	vim.ui.select = function(items, opts, callback)
		if opts.prompt == "Review" then
			assert(vim.deep_equal(items, { "working", "staged", "PR" }), "only requested review modes")
		end
		return select(items, opts, callback)
	end
	local function control(mode)
		local response = vim.system({
			"curl",
			"-sS",
			"-X",
			"POST",
			"-H",
			"Content-Type: application/json",
			"-d",
			vim.json.encode({ mode = mode }),
			vim.env.LGTM_E2E_GITHUB .. "/__control",
		}, { text = true }):wait()
		assert(response.code == 0)
		return vim.json.decode(response.stdout)
	end
	local function open(number)
		H.choice = function(items)
			for _, item in ipairs(items) do
				if tonumber(item.number) == number then
					return item
				end
			end
			error("Requested PR absent")
		end
		if number == 1 then
			H.manual_picker = true
		end
		local input = vim.ui.input
		vim.ui.input = function()
			error("Repository/search prompts must not precede inferred PR listing")
		end
		if number == 1 then
			H.choice = "PR"
			vim.cmd("Lgtm review")
		else
			vim.cmd("Lgtm prs")
		end
		if number == 1 then
			H.wait(function()
				local p = require("lgtm.picker").last
				return p and not p.closed
			end, "real PR search picker")
			local p = require("lgtm.picker").last
			assert(
				api.nvim_win_get_width(p.win) == 76 and api.nvim_win_get_height(p.list_win) == 10,
				"fixed-size PR picker"
			)
			H.keys("E2E pull 1")
			H.wait(function()
				return #p.filtered == 1 and tonumber(p.filtered[1].number) == 1
			end, "PR results filter as query is typed")
			H.choice = nil
			H.keys("<CR>")
			H.manual_picker = false
		end
		H.wait(function()
			local s = require("lgtm.context").current
			return s and tonumber(s.context.key.number) == number and s.panes ~= nil
		end, "PR picker and remote CodeDiff without checkout")
		vim.ui.input = input
		return require("lgtm.context").current
	end
	local function submit(outcome)
		H.choice = outcome
		vim.cmd("Lgtm submit")
		H.wait(function()
			local c = require("lgtm.composer").last
			return c and api.nvim_win_is_valid(c.win)
		end, "submission composer")
	end
	local state = open(1)
	assert(api.nvim_buf_get_lines(state.panes.old.buf, 1, 2, false)[1] == "old")
	assert(api.nvim_buf_get_lines(state.panes.new.buf, 1, 2, false)[1] == "new")
	api.nvim_set_current_win(state.panes.old.win)
	api.nvim_win_set_cursor(0, { 2, 0 })
	vim.cmd("Lgtm comment line")
	H.compose("Old-side inline feedback")
	api.nvim_set_current_win(state.panes.new.win)
	api.nvim_win_set_cursor(0, { 2, 0 })
	vim.cmd("Lgtm comment line")
	H.compose("New-side inline feedback")
	vim.cmd("Lgtm comment line")
	H.compose("Second thread on the new line")
	H.keys("<CR>")
	local popup = require("lgtm.threads").last
	local text = table.concat(api.nvim_buf_get_lines(popup.buf, 0, -1, false), "\n")
	assert(text:find("New-side inline feedback", 1, true) and text:find("Second thread on the new line", 1, true))
	H.keys("q")
	vim.cmd("Lgtm comment file")
	H.compose("Whole-file feedback")
	vim.cmd("Lgtm comment general")
	H.compose("General feedback")
	assert(#api.nvim_buf_get_extmarks(state.panes.old.buf, require("lgtm.threads").namespace, 0, -1, {}) == 1)
	assert(#api.nvim_buf_get_extmarks(state.panes.new.buf, require("lgtm.threads").namespace, 0, -1, {}) == 1)
	submit("approve")
	H.compose("Final approval body")
	assert(state.session.submissionStatus == "confirmed")
	local status = control("normal")
	assert(status.writes == 7, "one pending review, five comments, one final event")
	local found
	for _, notification in ipairs(require("lgtm.notifications").history) do
		if notification.message == "Review submitted" then
			found = true
		end
	end
	assert(found, "success notification after confirmation")
	vim.cmd("Lgtm close")
	state = open(2)
	vim.cmd("Lgtm comment general")
	H.compose("Retained draft on refresh failure")
	control("denied")
	local notices = #require("lgtm.notifications").history
	vim.cmd("Lgtm refresh")
	H.wait(function()
		return #require("lgtm.notifications").history > notices + 1
	end, "permission failure notification")
	assert(state.conversation.comments[1].body == "Retained draft on refresh failure")

	control("stale")
	submit("request_changes")
	local composer = require("lgtm.composer").last
	api.nvim_buf_set_lines(composer.buf, 0, -1, false, { "Retained final body after stale head" })
	local before = #require("lgtm.notifications").history
	H.keys("<C-s>")
	H.wait(function()
		return #require("lgtm.notifications").history > before + 1
	end, "stale-head failure notification")
	assert(api.nvim_win_is_valid(composer.win), "failed submission retains composer")
	assert(api.nvim_buf_get_lines(composer.buf, 0, 1, false)[1] == "Retained final body after stale head")
	H.keys("q")
	assert(control("normal").writes == 7, "stale head does not publish")
	vim.cmd("Lgtm close")
	state = open(3)
	control("lost")
	submit("comment")
	composer = require("lgtm.composer").last
	api.nvim_buf_set_lines(composer.buf, 0, -1, false, { "Uncertain final body" })
	H.keys("<C-s>")
	H.wait(function()
		return state.session.submissionStatus == "uncertain"
	end, "durable uncertain state")
	assert(api.nvim_win_is_valid(composer.win))
	H.keys("q")
	local writes = control("normal").writes
	vim.cmd("Lgtm reconcile")
	H.wait(function()
		return state.session.submissionStatus == "confirmed"
	end, "reconciliation confirms accepted remote result")
	assert(control("normal").writes == writes, "reconciliation never replays writes")
	local exported = H.rpc("Export", { sessionId = state.session.id })
	assert(exported.markdown:find("Uncertain final body", 1, true))
	vim.cmd("Lgtm close")
end

return should_support_github_review
