local suite = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
package.path = suite .. "/?.lua;" .. suite .. "/common/?.lua;" .. package.path
local H = {}
local test
local function drive_picker()
	local picker = require("lgtm.picker").last
	if H.manual_picker or not picker or picker.closed or picker.driven then
		return
	end
	picker.driven = true
	local choice = H.choices and table.remove(H.choices, 1) or H.choice
	H.choice = nil
	local item = picker.filtered[1]
	if type(choice) == "function" then
		item = choice(picker.filtered, { prompt = "picker" })
	elseif choice then
		for _, entry in ipairs(picker.filtered) do
			if entry == choice then
				item = entry
			end
		end
	end
	for index, entry in ipairs(picker.filtered) do
		if entry == item then
			vim.api.nvim_set_current_win(picker.list_win)
			vim.api.nvim_win_set_cursor(picker.list_win, { index, 0 })
			vim.api.nvim_input("<CR>")
			return
		end
	end
end
local function resume()
	local ok, error = coroutine.resume(test)
	if not ok then
		io.stderr:write(tostring(error) .. "\n")
		vim.cmd("cquit 1")
	end
end
function H.wait(predicate, description)
	local deadline = vim.uv.hrtime() + 15e9
	while not predicate() do
		drive_picker()
		assert(
			vim.uv.hrtime() < deadline,
			"Timed out: " .. description .. "\n" .. vim.inspect(require("lgtm.notifications").history)
		)
		vim.defer_fn(resume, 10)
		coroutine.yield()
	end
end
function H.rpc(method, input)
	local done, err, result = false, nil, nil
	require("lgtm.rpc").call(method, input, function(error, value)
		done = true
		err = error
		result = value
	end)
	H.wait(function()
		return done
	end, method)
	assert(not err, method .. ": " .. vim.inspect(err))
	return result
end
function H.keys(keys)
	print("Driving " .. keys)
	vim.api.nvim_input(keys)
	vim.defer_fn(resume, 80)
	coroutine.yield()
end
function H.compose(body)
	H.wait(function()
		local c = require("lgtm.composer").last
		return c and vim.api.nvim_win_is_valid(c.win)
	end, "composer")
	local c = require("lgtm.composer").last
	assert(vim.api.nvim_win_get_width(c.win) == 80 and vim.api.nvim_win_get_height(c.win) == 12, "fixed-size composer")
	vim.api.nvim_buf_set_lines(c.buf, 0, -1, false, vim.split(body, "\n", { plain = true }))
	H.keys("<C-s>")
	H.wait(function()
		return not vim.api.nvim_win_is_valid(c.win)
	end, "saved composer")
end
H.choice = nil
vim.ui.select = function(items, opts, cb)
	local choice = H.choices and table.remove(H.choices, 1) or H.choice
	H.choice = nil
	local item = items[1]
	if type(choice) == "function" then
		item = choice(items, opts)
	elseif choice then
		for _, entry in ipairs(items) do
			if entry == choice then
				item = entry
			end
		end
	end
	vim.schedule(function()
		cb(item)
	end)
end
vim.ui.input = function(opts, cb)
	vim.schedule(function()
		cb(opts.default or "")
	end)
end
local case = vim.g.lgtm_case or "workflow"
test = coroutine.create(function()
	local ok, error = xpcall(function()
		require(case)(H)
	end, debug.traceback)
	if not ok then
		io.stderr:write(error .. "\n")
		vim.cmd("cquit 1")
	else
		print("PASS Neovim " .. case)
		vim.cmd("qa!")
	end
end)
vim.schedule(resume)
