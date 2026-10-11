-- Protobuf JSON adapter for the generated Connect endpoints. Neovim uses curl
-- rather than a native HTTP/2/protobuf runtime; the server proxies these typed
-- methods through its generated Go gRPC client.
local M = {}
local function actor(value)
  if not value then return nil end
  return { name = value.name, kind = value.kind == 'agent' and 'ACTOR_KIND_AGENT' or 'ACTOR_KIND_HUMAN' }
end
function M.request(q)
  local method, message
  if q.op == 'state' then method = 'GetWorkspace'; message = vim.empty_dict()
  elseif q.op == 'pulls' then method = 'ListPullRequests'; message = vim.empty_dict()
  elseif q.op == 'files' then method = 'ListFiles'; message = { reviewId = q.id, includeUnchanged = q.kind == 'all' }
  elseif q.op == 'diff' then method = 'GetDiff'; message = { reviewId = q.id, path = q.path }
  elseif q.op == 'file' then method = 'GetFile'; message = { path = q.path }
  elseif q.op == 'history' then method = 'GetHistory'; message = { reviewId = q.id }
  elseif q.op == 'create' then method = 'CreateReview'; message = { kind = q.kind == 'github' and 'REVIEW_KIND_GITHUB' or 'REVIEW_KIND_LOCAL', actor = actor(q.actor), body = q.body, pr = q.pr, repository = q.repo }
  elseif q.op == 'comment' then method = 'AddComment'; message = { path = q.path, line = q.line, side = q.side == 'LEFT' and 'DIFF_SIDE_LEFT' or 'DIFF_SIDE_RIGHT', body = q.body, actor = actor(q.actor), reviewId = q.reviewId }
  elseif q.op == 'reply' then method = 'AddReply'; message = { threadId = q.id, body = q.body, actor = actor(q.actor) }
  elseif q.op == 'resolve' then method = 'SetThreadResolved'; message = { threadId = q.id, resolved = q.event ~= 'REOPEN' }
  elseif q.op == 'promote' then method = 'PromoteThread'; message = { threadId = q.id, reviewId = q.reviewId }
  elseif q.op == 'edit' then method = 'UpdateReview'; message = { reviewId = q.id, body = q.body }
  elseif q.op == 'submit' then method = 'SubmitReview'; message = { reviewId = q.id, event = 'REVIEW_EVENT_' .. (q.event or 'UNSPECIFIED') }
  else error('Unknown LGTM operation: ' .. tostring(q.op)) end
  return '/lgtm.v1.ReviewService/' .. method, message
end
local function actor_from_proto(value)
  value = value or {}
  return { name = value.name or '', kind = value.kind == 'ACTOR_KIND_AGENT' and 'agent' or 'human' }
end
function M.response(out)
  local workspace = out.workspace or {}
  local state = workspace.state or {}
  state.reviews = state.reviews or {}; state.comments = state.comments or {}
  for _, r in ipairs(state.reviews) do
    r.kind = r.kind == 'REVIEW_KIND_GITHUB' and 'github' or 'local'
    r.status = (r.status or ''):gsub('^REVIEW_STATUS_', ''); if r.status == 'DRAFT' then r.status = 'draft' end
    r.repo = r.repo or ''; r.pr = r.pr or 0; r.body = r.body or ''; r.head = r.head or ''; r.remoteId = tonumber(r.remoteId or 0)
    r.actor = actor_from_proto(r.actor)
  end
  for _, c in ipairs(state.comments) do
    c.side = c.side == 'DIFF_SIDE_LEFT' and 'LEFT' or 'RIGHT'
    c.body = c.body or ''; c.reviewId = c.reviewId or ''; c.actor = actor_from_proto(c.actor)
    if c.threadId == '' then c.threadId = nil end
  end
  return { state = state, repoPath = workspace.repoPath, text = out.text, files = out.files or {}, repository = out.repository, pulls = out.pulls or {} }
end
return M
