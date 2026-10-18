local suite = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
for _, file in ipairs(vim.fn.glob(suite .. "/**/*.lua", false, true)) do
	if not file:find("/.cache/", 1, true) then
		assert(loadfile(file))
	end
end
print("Neovim E2E syntax passed")
vim.cmd("qa!")
