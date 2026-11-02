local PrService = require("lgtm.services.pr_service")

local M = {}

-- fixtures --------------------------------------------------------------------

local function describe(_title, fn)
  fn()
end

local function it(_title, fn)
  fn()
end

local function make_pr_service()
  return PrService.new({
    seed_prs = {
      {
        number = 101,
        title = "Scaffold reactive Neovim UI",
        description = "Adds PR picker, CodeDiff session, and threads pane.",
        author = "siddharth",
        source_branch = "feature/nvim-ui-scaffolding",
        target_branch = "main",
        state = "open",
        lines_added = 340,
        lines_removed = 12,
        files = {
          { path = "apps/lgtm.nvim/lua/lgtm/init.lua", status = "M" },
        },
      },
      {
        number = 102,
        title = "Other branch PR",
        description = "Unrelated branch work.",
        author = "teammate",
        source_branch = "feature/other",
        target_branch = "main",
        state = "draft",
      },
    },
  })
end

-- tests -----------------------------------------------------------------------

function M.run()
  describe("When selecting pull requests and managing PR threads in PrService", function()
    it("should_manage_pr_selection_isolated_pr_threads_and_review_submissions", function()
      local service = make_pr_service()
      local prs, err = service:get_open_prs("feature/nvim-ui-scaffolding")
      assert(err == nil)
      assert(#prs == 1 and prs[1].number == 101)

      local selected = service:select_pr(101)
      assert(selected.number == 101)
      assert(service.state:get_value().selected_pr.number == 101)

      local thread = service:create_thread({
        path = "apps/lgtm.nvim/lua/lgtm/init.lua",
        side = "RIGHT",
        line = 8,
        body = "Verify dependency load order.",
      })
      assert(thread.mode == "pr" and thread.pr_number == 101)
      assert(thread.comments[1].state == "PENDING")

      service:reply_to_thread(thread.id, "Checked and verified.")
      assert(#service:get_thread(thread.id).comments == 2)

      assert(service:toggle_file_reviewed("apps/lgtm.nvim/lua/lgtm/init.lua") == true)
      assert(service:toggle_file_reviewed("apps/lgtm.nvim/lua/lgtm/init.lua") == false)

      local submission = service:submit_review({
        outcome = "comment",
        body = "Looks good overall, left one inline note.",
      })
      assert(submission.pr_number == 101)
      assert(submission.outcome == "comment")
      assert(#service.state:get_value().submissions == 1)
      assert(service:get_thread(thread.id).comments[1].state == "PUBLISHED")

      service:select_pr(102)
      assert(#service:get_threads() == 0, "PR #102 threads must be isolated from PR #101")
      service:select_pr(101)
      assert(#service:get_threads() == 1, "PR #101 threads must persist in memory across switches")
    end)
  end)
end

return M
