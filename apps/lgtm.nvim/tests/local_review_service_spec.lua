local LocalReviewService = require("lgtm.services.local_review_service")

local M = {}

-- fixtures --------------------------------------------------------------------

local function describe(_title, fn)
  fn()
end

local function it(_title, fn)
  fn()
end

local function make_service()
  return LocalReviewService.new()
end

-- tests -----------------------------------------------------------------------

function M.run()
  describe("When mutating always-on local review threads in LocalReviewService", function()
    it(
      "should_emit_signal_updates_across_local_thread_create_reply_edit_delete_collapse_and_resolve",
      function()
        local service = make_service()
        local observed_counts = {}
        local subscription = service.state.threads:observe(function(threads)
          table.insert(observed_counts, #threads)
        end)

        local created = service:create_thread({
          path = "lua/lgtm/init.lua",
          start_line = 4,
          line = 6,
          body = "Consider extracting setup options.",
          author = "reviewer",
        })
        assert(created.mode == "local")
        assert(#service:get_threads() == 1)
        assert(service.state:get_value().selected_thread_id == created.id)

        local _, reply = service:reply_to_thread(created.id, "Agreed, will extract.", "author")
        assert(#service:get_thread(created.id).comments == 2)

        service:edit_comment(created.id, reply.id, "Agreed, extracted to config.lua.")
        assert(
          service:get_thread(created.id).comments[2].body == "Agreed, extracted to config.lua."
        )

        local collapsed = service:toggle_collapsed(created.id)
        assert(collapsed.collapsed == true)
        service:set_all_collapsed(false)
        assert(service:get_thread(created.id).collapsed == false)

        local resolved = service:toggle_resolved(created.id)
        assert(resolved.resolved == true)
        assert(#service:get_threads({ include_resolved = false }) == 0)

        service:open_thread_detail(created.id)
        assert(service.state:get_value().active_view == "detail")
        service:close_thread_detail()
        assert(service.state:get_value().active_view == "summary")

        service:delete_comment(created.id, reply.id)
        assert(#service:get_thread(created.id).comments == 1)
        service:delete_comment(created.id, created.comments[1].id)
        assert(#service:get_threads() == 0)

        subscription:unsubscribe()
        assert(#observed_counts >= 6)
      end
    )
  end)
end

return M
