local M = {}
function M.setup()
  local prefix = require("lgtm.config").options.prefix
  local groups = {
    { prefix, group = "LGTM review" },
    {
      prefix .. "b",
      function()
        require("lgtm.review").branch("branch")
      end,
      desc = "Review branch",
    },
    {
      prefix .. "e",
      function()
        require("lgtm.review").comment_action("e")
      end,
      desc = "Edit comment",
    },
    {
      prefix .. "d",
      function()
        require("lgtm.review").comment_action("dd")
      end,
      desc = "Delete comment",
    },
    {
      prefix .. "r",
      function()
        require("lgtm.review").branch("working")
      end,
      desc = "Review working tree",
    },
    {
      prefix .. "p",
      function()
        require("lgtm.sessions").prs()
      end,
      desc = "Pull requests",
    },
    {
      prefix .. "S",
      function()
        require("lgtm.sessions").select()
      end,
      desc = "Resume session",
    },
    {
      prefix .. "D",
      function()
        require("lgtm.review").directory()
      end,
      desc = "Review directory",
    },
    {
      prefix .. "n",
      ":Lgtm note<CR>",
      desc = "Add note",
      mode = { "n", "x" },
    },
    {
      prefix .. "c",
      function()
        require("lgtm.review").comment("line")
      end,
      desc = "Add comment",
      mode = { "n", "x" },
    },
    {
      prefix .. "f",
      function()
        require("lgtm.review").comment("file")
      end,
      desc = "File comment",
    },
    {
      prefix .. "g",
      function()
        require("lgtm.review").comment("general")
      end,
      desc = "General comment",
    },
    {
      prefix .. "t",
      function()
        require("lgtm.review").threads()
      end,
      desc = "All threads",
    },
    {
      prefix .. "F",
      function()
        require("lgtm.review").files()
      end,
      desc = "Review files",
    },
    {
      prefix .. "m",
      function()
        require("lgtm.review").reviewed()
      end,
      desc = "Toggle file reviewed",
    },
    {
      prefix .. "P",
      function()
        require("lgtm.notes").promote()
      end,
      desc = "Promote note",
    },
    {
      prefix .. "x",
      function()
        require("lgtm.export").run(require("lgtm.context").current and "review" or "notes")
      end,
      desc = "Export review",
    },
    {
      prefix .. "s",
      function()
        require("lgtm.review").submit()
      end,
      desc = "Submit review",
    },
    {
      prefix .. "y",
      function()
        require("lgtm.review").sync()
      end,
      desc = "Sync GitHub drafts",
    },
    {
      prefix .. "q",
      function()
        require("lgtm.review").close()
      end,
      desc = "Close review view",
    },
  }
  require("which-key").add(groups)
end
return M
