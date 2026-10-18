-- tests -----------------------------------------------------------------------
local function should_check_real_backend_and_dependencies(H)
	vim.cmd("checkhealth lgtm")
	H.wait(function()
		return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Backend reachable, API 1", 1, true)
	end, "health checks real Connect endpoint")
	local report = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
	assert(report:find("CodeDiff native library loaded", 1, true))
	assert(not report:find("ERROR", 1, true), report)
end

return should_check_real_backend_and_dependencies
