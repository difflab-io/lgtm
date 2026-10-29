-- State -----------------------------------------------------------------------

local M = {}
local UI, Private = {}, {}

-- Render ----------------------------------------------------------------------

function M.render(props)
  local nodes = { UI.header(props) }
  return vim.list_extend(
    nodes,
    vim.tbl_map(function(comment)
      return UI.comment({
        thread_id = props.thread.id,
        comment = comment,
        can_resolve = props.can_resolve,
      })
    end, props.comments)
  )
end

-- UI --------------------------------------------------------------------------

UI.comment = require("lgtm.features.thread_comment").render

function UI.header(props)
  local thread = props.thread
  local rows = {
    { " " .. Private.heading(props), thread.state == "resolved" and "LgtmSuccess" or "LgtmInfo" },
    { string.rep("─", props.width), "LgtmBorder" },
  }
  for _, detail in ipairs(Private.details(thread, props.reason)) do
    rows[#rows + 1] = { " " .. detail, "LgtmWarning" }
  end
  return require("nui.tree").Node({
    id = "thread:" .. thread.id,
    thread_id = thread.id,
    lines = rows,
  })
end

-- Private ---------------------------------------------------------------------

function Private.heading(props)
  local thread, anchor = props.thread, props.thread.anchor or {}
  local replies = math.max(0, #props.comments - 1)
  local heading = (thread.state == "resolved" and "RESOLVED" or "OPEN")
    .. " · "
    .. replies
    .. (replies == 1 and " reply" or " replies")
  if props.count > 1 then
    heading = "THREAD " .. props.index .. " / " .. props.count .. " · " .. heading
  end
  if props.mode == "split" then
    heading = heading
      .. " · "
      .. (anchor.path or "Review")
      .. (anchor.start and ":" .. anchor.start or "")
  end
  if props.location_status then
    heading = props.location_status:upper() .. " · " .. heading
  end
  return heading
end

function Private.details(thread, reason)
  if not reason then
    return {}
  end
  local anchor = thread.anchor or {}
  local rows = { reason }
  if anchor.start then
    rows[#rows + 1] = "Original lines " .. anchor.start .. "–" .. anchor["end"]
  end
  local excerpt = anchor.evidence and anchor.evidence.selectedText
  if excerpt and excerpt ~= "" then
    for _, line in ipairs(vim.list_slice(vim.split(excerpt, "\n", { plain = true }), 1, 6)) do
      rows[#rows + 1] = "> " .. line
    end
  end
  return rows
end
return M
