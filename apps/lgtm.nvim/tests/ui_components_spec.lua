local AnchoredThreadSummary = require("lgtm.ui.anchored_thread_summary")
local Thread = require("lgtm.models.thread")
local ThreadDetailView = require("lgtm.ui.thread_detail_view")
local ThreadSummaryView = require("lgtm.ui.thread_summary_view")
local n = require("nui-components")

local M = {}

local function describe(_title, fn)
  fn()
end

local function it(_title, fn)
  fn()
end

local function make_thread(overrides)
  return Thread.new(vim.tbl_extend("force", {
    id = "thread-ui-1",
    mode = "local",
    path = "src/service.lua",
    line = 14,
    start_line = 12,
    author = "alice",
    body = "Ensure full-width boxed virtual lines render with colored borders.",
  }, overrides or {}))
end

local function lines_to_text(lines)
  local out = {}
  for _, line in ipairs(lines) do
    table.insert(out, line:content())
  end
  return table.concat(out, "\n")
end

function M.run()
  describe("When rendering boxed AnchoredThreadSummary cards and Component views", function()
    it(
      "should_render_full_width_boxed_virtual_lines_and_extend_nuicomponents_for_summary_and_detail_views",
      function()
        local thread = make_thread()
        local with_reply = Thread.with_reply(thread, {
          id = "comment-ui-2",
          author = "bob",
          body = "Confirmed with rounded box borders and author highlights.",
        })

        local card = AnchoredThreadSummary(with_reply, {
          is_selected = true,
          width = 80,
        })
        assert(card.border_hl == "LgtmThreadBorderSelected")
        assert(card.sign_hl == "LgtmThreadSignSelected")

        local virt_lines = card:render()
        assert(
          #virt_lines >= 6,
          "expanded card with reply must include border, header, body, and footer rows"
        )
        local top_text = virt_lines[1][1][1]
        local bottom_text = virt_lines[#virt_lines][1][1]
        assert(top_text:find("╭", 1, true) ~= nil and top_text:find("╮", 1, true) ~= nil)
        assert(bottom_text:find("╰", 1, true) ~= nil and bottom_text:find("╯", 1, true) ~= nil)
        assert(vim.fn.strdisplaywidth(top_text) == 79)
        assert(vim.fn.strdisplaywidth(bottom_text) == 79)

        local collapsed_thread = Thread.with_collapsed(with_reply, true)
        local collapsed_lines = AnchoredThreadSummary.render_virt_lines(collapsed_thread, {
          width = 80,
        })
        assert(
          #collapsed_lines == 3,
          "collapsed card must only render top border, header, and bottom border"
        )

        local signal = n.create_signal({
          threads = { with_reply },
          selected_thread_id = with_reply.id,
        })

        local summary_cmp = ThreadSummaryView({
          id = "spec_summary_view",
          mode = "local",
          threads = signal.threads:dup(),
          selected_thread_id = signal.selected_thread_id:dup(),
        })
        local summary_lines = summary_cmp:get_lines()
        local summary_text = lines_to_text(summary_lines)
        assert(summary_text:find("Local Review Threads (1)", 1, true) ~= nil)
        assert(summary_text:find("src/service.lua:12-14", 1, true) ~= nil)

        local detail_cmp = ThreadDetailView({
          id = "spec_detail_view",
          threads = signal.threads:dup(),
          selected_thread_id = signal.selected_thread_id:dup(),
        })
        local detail_lines = detail_cmp:get_lines()
        local detail_text = lines_to_text(detail_lines)
        assert(detail_text:find("Thread Detail · [OPEN] src/service.lua:12-14", 1, true) ~= nil)
        assert(detail_text:find("Confirmed with rounded box borders", 1, true) ~= nil)
      end
    )
  end)
end

return M
