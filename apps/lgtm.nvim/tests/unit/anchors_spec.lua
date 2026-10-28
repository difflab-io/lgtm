-- fixtures --------------------------------------------------------------------

local anchors = require("lgtm.anchors")
local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h:h")
local path = root .. "/../../e2e/common/anchor_cases.json"
local cases = vim.json.decode(table.concat(vim.fn.readfile(path), "\n"))
local function optional(value)
  return value ~= vim.NIL and value or nil
end

-- unit tests ------------------------------------------------------------------

describe("When placing recorded source anchors", function()
  for _, case in ipairs(cases) do
    it(case.name, function()
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
    end)
  end
end)

describe("When viewing historical provider feedback", function()
  it("Preserves provider status while checking exact saved-source evidence", function()
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
    local unproven = vim.deepcopy(thread)
    unproven.anchor.source, unproven.anchor.evidence = nil, nil
    assert(
      anchors.resolve(unproven, view).status == "unverified",
      "missing saved-byte evidence is not historical proof"
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
  end)
end)
