local M = {}

---Load the retained dependencies without starting any LGTM features.
function M.setup(_opts)
  require("codediff")
  require("which-key")
  require("notify")
  require("nui-components")
  require("nui.popup")
  pcall(require, "kak.keybinds")
end

return M
