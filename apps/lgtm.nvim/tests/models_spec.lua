local PullRequest = require("lgtm.models.pull_request")
local ReviewSubmission = require("lgtm.models.review_submission")
local Thread = require("lgtm.models.thread")

local M = {}

-- fixtures --------------------------------------------------------------------

local function describe(_title, fn)
  fn()
end

local function it(_title, fn)
  fn()
end

local function make_thread(overrides)
  return Thread.new(vim.tbl_extend("force", {
    id = "thread-1",
    mode = "local",
    path = "./src/main.lua",
    line = 12,
    start_line = 10,
    author = "alice",
    body = "Initial observation",
  }, overrides or {}))
end

-- tests -----------------------------------------------------------------------

function M.run()
  describe("When validating and transforming Thread and Comment models", function()
    it("should_validate_and_normalize_thread_comment_pull_request_and_submission_models", function()
      local thread = make_thread()
      assert(thread.path == "src/main.lua")
      assert(thread.start_line == 10 and thread.line == 12)
      assert(Thread.format_anchor(thread) == "src/main.lua:10-12")
      assert(#thread.comments == 1)
      assert(thread.comments[1].state == "DRAFT")

      local with_reply, reply = Thread.with_reply(thread, {
        id = "comment-2",
        author = "bob",
        body = "Follow-up reply",
      })
      assert(#thread.comments == 1, "original thread must remain immutable")
      assert(#with_reply.comments == 2)
      assert(reply.author == "bob")
      local root, replies = Thread.summary_stats(with_reply)
      assert(root.body == "Initial observation" and replies == 1)

      local edited = Thread.with_edited_comment(with_reply, "comment-2", "Updated reply")
      assert(edited.comments[2].body == "Updated reply")

      local resolved = Thread.with_resolved(edited, true)
      assert(resolved.resolved == true and edited.resolved == false)

      local collapsed = Thread.with_collapsed(resolved, true)
      assert(collapsed.collapsed == true and resolved.collapsed == false)

      local after_remove = Thread.without_comment(collapsed, "comment-2")
      assert(after_remove and #after_remove.comments == 1)
      local empty = Thread.without_comment(after_remove, after_remove.comments[1].id)
      assert(empty == nil)

      local pr = PullRequest.new({
        number = 42,
        title = "Add reactive UI scaffolding",
        body = "Implements local and PR review workflows.\r\n",
        author = { login = "octocat" },
        headRefName = "feature/nvim-ui-scaffolding",
        baseRefName = "main",
        isDraft = true,
        additions = 120,
        deletions = 15,
        files = {
          { path = "lua/lgtm/init.lua", status = "M" },
        },
      })
      assert(pr.number == 42)
      assert(pr.author == "octocat")
      assert(pr.state == "draft")
      assert(PullRequest.matches_query(pr, "scaffolding"))
      assert(PullRequest.matches_query(pr, "#42"))
      assert(not PullRequest.matches_query(pr, "nonexistent"))
      local preview = table.concat(PullRequest.format_preview_lines(pr), "\n")
      assert(preview:find("Implements local and PR review workflows.", 1, true) ~= nil)

      local submission = ReviewSubmission.new({
        pr_number = 42,
        outcome = "approve",
        body = "",
      })
      assert(submission.outcome == "approve")
      assert(ReviewSubmission.outcome_label("request_changes") == "Request Changes")
    end)
  end)
end

return M
