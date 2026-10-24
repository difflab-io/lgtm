-- State -----------------------------------------------------------------------

local M = {}
local rpc = require("lgtm.services.client")

-- Public ----------------------------------------------------------------------

function M.get_reviews(completed, callback)
  local reviews = {}
  local function page(token)
    rpc.latest(
      "review-picker",
      "ListReviews",
      { state = completed and "completed" or "active", pageToken = token, pageSize = 100 },
      function(err, result)
        if err then
          callback(err)
          return
        end
        vim.list_extend(reviews, result.reviews or {})
        if result.nextPageToken and result.nextPageToken ~= "" then
          page(result.nextPageToken)
        else
          callback(nil, reviews)
        end
      end
    )
  end
  page(nil)
end

function M.get_pull_requests(url, query, page, callback)
  rpc.latest(
    "pr-picker",
    "ListPullRequests",
    { repositoryUrl = url, query = query, page = page },
    function(err, result)
      if err then
        callback(err)
        return
      end
      local entries = result.pullRequests or {}
      if (result.nextPage or 0) > 0 then
        entries[#entries + 1] = { nextPage = result.nextPage }
      end
      callback(nil, entries)
    end
  )
end

return M
