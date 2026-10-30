-- State -----------------------------------------------------------------------

local M = {}
local UI = {}

-- Render ----------------------------------------------------------------------

function M.render(props)
  local comment = props.comment
  local root = tonumber(comment.position or 0) == 0
  local role = root and "top-level comment" or "reply " .. comment.position
  return require("nui.tree").Node({
    id = "comment:" .. comment.id,
    thread_id = props.thread_id,
    comment_id = comment.id,
    role = role,
    can_resolve = props.can_resolve,
    lines = vim.list_extend(UI.header(comment, root, role), UI.body(comment, root)),
  })
end

-- UI --------------------------------------------------------------------------

function UI.header(comment, root, role)
  local prefix = root and " " or "   └─ "
  return {
    {
      prefix
        .. role:upper()
        .. " · "
        .. (comment.author and comment.author.name or "Unknown")
        .. " · "
        .. M.get_status(comment),
      root and "LgtmTitle" or "LgtmMuted",
    },
  }
end

function UI.body(comment, root)
  local rows = vim.tbl_map(function(line)
    return { (root and "   " or "      ") .. line, "LgtmNormal" }
  end, vim.split(comment.body, "\n", { plain = true }))
  rows[#rows + 1] = { "", "LgtmNormal" }
  return rows
end

-- Methods ---------------------------------------------------------------------

function M.get_status(comment)
  if comment.visibility ~= "draft" then
    return comment.visibility or ""
  end
  if comment.postingStatus == "uncertain" or comment.postingStatus == "sending" then
    return "Sync uncertain"
  end
  return comment.forgeCommentId and comment.forgeCommentId ~= "" and "GitHub draft" or "Local draft"
end
return M
