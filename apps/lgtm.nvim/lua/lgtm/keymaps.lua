-- State -----------------------------------------------------------------------

local M = {}

-- Methods ---------------------------------------------------------------------

function M.setup()
  local prefix = require("lgtm.config").options.prefix
  local groups = {
    { prefix, group = "LGTM review" },
    {
      prefix .. "b",
      function()
        require("lgtm.features.new_review_dialog").local_review("branch")
      end,
      desc = "Review branch",
    },
    {
      prefix .. "e",
      function()
        require("lgtm.app").comment_action("e")
      end,
      desc = "Edit comment",
    },
    {
      prefix .. "d",
      function()
        require("lgtm.app").comment_action("dd")
      end,
      desc = "Delete comment",
    },
    {
      prefix .. "r",
      function()
        require("lgtm.features.new_review_dialog").local_review("working")
      end,
      desc = "Review working tree",
    },
    {
      prefix .. "p",
      function()
        require("lgtm.features.branch_pr_selector").open()
      end,
      desc = "Pull requests",
    },
    {
      prefix .. "S",
      function()
        require("lgtm.features.session_selector").select()
      end,
      desc = "Resume session",
    },
    {
      prefix .. "D",
      function()
        require("lgtm.features.new_review_dialog").directory()
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
        require("lgtm.app").comment("line")
      end,
      desc = "Add comment",
      mode = { "n", "x" },
    },
    {
      prefix .. "f",
      function()
        require("lgtm.app").comment("file")
      end,
      desc = "File comment",
    },
    {
      prefix .. "g",
      function()
        require("lgtm.app").comment("general")
      end,
      desc = "General comment",
    },
    {
      prefix .. "t",
      function()
        require("lgtm.features.threads_pane").open()
      end,
      desc = "All threads",
    },
    {
      prefix .. "F",
      function()
        require("lgtm.app").files()
      end,
      desc = "Review files",
    },
    {
      prefix .. "m",
      function()
        require("lgtm.app").reviewed()
      end,
      desc = "Toggle file reviewed",
    },
    {
      prefix .. "P",
      function()
        require("lgtm.app").promote_note()
      end,
      desc = "Promote note",
    },
    {
      prefix .. "x",
      function()
        require("lgtm.app").export(require("lgtm.services.state").current and "review" or "notes")
      end,
      desc = "Export review",
    },
    {
      prefix .. "s",
      function()
        require("lgtm.app").submit()
      end,
      desc = "Submit review",
    },
    {
      prefix .. "y",
      function()
        require("lgtm.app").sync()
      end,
      desc = "Sync GitHub drafts",
    },
    {
      prefix .. "q",
      function()
        require("lgtm.app").close()
      end,
      desc = "Close review view",
    },
  }
  require("which-key").add(groups)
end
return M
