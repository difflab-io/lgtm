-- Shared behavioral cases; no UI mocks or alternate placement implementation.
return function(root)
  local anchors = require("lgtm.anchors")
  local path = root .. "/../../e2e/common/anchor_cases.json"
  local cases = vim.json.decode(table.concat(vim.fn.readfile(path), "\n"))
  local function optional(value)
    return value ~= vim.NIL and value or nil
  end
  for _, case in ipairs(cases) do
    local original = vim.deepcopy(case)
    local actual = anchors.place(
      case.anchor,
      optional(case.evidence),
      case.path,
      optional(case.displayed_source),
      optional(case.content),
      case.path_confirmed
    )
    assert(actual.status == case.expected.status, case.name .. ": " .. vim.inspect(actual))
    for _, field in ipairs({ "path", "start", "end", "kind" }) do
      if case.expected[field] then
        assert(
          actual.location[field] == case.expected[field],
          case.name .. ": " .. vim.inspect(actual)
        )
      end
    end
    assert(vim.deep_equal(case, original), case.name .. " mutated its original")
  end
  -- Provider state is independent of source-bound historical placement.
  local content = "before\nselected\nafter\n"
  local sha, other = string.rep("a", 40), string.rep("b", 40)
  local hash = "sha256:" .. vim.fn.sha256(content)
  local thread = {
    anchor = {
      kind = "line",
      path = "file.md",
      side = "new",
      start = "2",
      ["end"] = "2",
      source = { commit = { sha = sha, contentHash = hash } },
      evidence = { selectedText = "selected", contextBefore = "before", contextAfter = "after" },
    },
    currentPath = "renamed.md",
    forgeObservation = { base = other, head = sha, outdated = true },
  }
  local original = vim.deepcopy(thread)
  local view = {
    path = "file.md",
    logical_path = "file.md",
    side = "new",
    source = { kind = "commit", sha = sha, content_hash = hash },
    content = content,
    comparison = { pullRequest = { baseCommit = other, headCommit = sha } },
  }
  assert(
    anchors.resolve(thread, view).status == "outdated",
    "live provider-outdated cannot map through matching text"
  )
  view.historical = true
  assert(
    anchors.resolve(thread, view).status == "exact",
    "the typed original commit independently confirms historical placement after a move"
  )
  local legacy = vim.deepcopy(thread)
  legacy.anchor.source, legacy.anchor.evidence = nil, nil
  legacy.anchor.commit = sha
  assert(
    anchors.resolve(legacy, view).status == "unverified",
    "a legacy hash or commit field is not historical proof"
  )
  view.unavailable = "Source could not be read"
  assert(
    anchors.resolve(thread, view).status == "unverified",
    "a failed source read is not deletion"
  )
  assert(
    vim.deep_equal(original, thread),
    "viewing historical placement never clears provider-outdated state or changes the original"
  )
  print("Shared source/placement cases passed: " .. #cases)
end
