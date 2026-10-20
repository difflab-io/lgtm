# End-to-end tests

Each suite owns its Mise config. There is no `e2e/mise.toml`.

| Suite | Language | Entry point | Run |
| --- | --- | --- | --- |
| `backend/` | Rust | Actual standalone backend process and public Connect RPCs | `premise run e2e:backend` |
| `cli/` | Rust | Built CLI commands, bundled backend and database | `premise run e2e:cli` |
| `nvim/` | Lua | Child Neovim with real CodeDiff, WhichKey, notifications and backend | `premise run e2e:nvim` |

`premise run e2e` runs all suites. Tests isolate runtime directories, databases and repositories and clean up child processes. Component integration tests remain under each app/library's `tests/` directory. GitHub HTTP fixtures make normal tests deterministic; they do not prove live GitHub behavior. Any live test repositories must be private, with visibility verified before fixture content is uploaded. If private creation is unavailable, report the live test skipped rather than create a public repository.

Premise currently only accepts registered projects under `apps/` and `libs/`. Existing root tasks delegate to these E2E Mise configs until [CLO-187](https://linear.app/cloudvoyant/issue/CLO-187/support-top-level-e2e-projects-with-independent-mise-configs-and-ci) adds native support.

To run the opt-in live GitHub smoke, use `LGTM_LIVE_GITHUB_E2E=1 premise run e2e:github:live`. It uses local `gh` credentials, verifies private visibility before uploading content, and closes the PR/archives the private repository afterward. It never runs in ordinary CI.

The Neovim suite runs two isolated configurations. The required configuration
pins CodeDiff, WhichKey, and nvim-notify. The compatibility configuration adds
Kak. Kak is a test dependency only and is not required by lgtm.nvim. Both
configurations use the real backend and drive editor keys.

The native Git fixture verifies the CodeDiff explorer tree, Git revision
buffers, arrow-key PR selection, local branch review, and Working/Staged changes.
It also verifies that a local PR review downloads no GitHub file contents.
Remote snapshot buffers must have a non-file buffer type to prevent accidental
language-server startup.

Each editor journey prints named `SCENARIO` and `PASS` records. A failure names
the active user flow before its Lua stack trace. The native Git fixture delays
GitHub comments and verifies that the tree opens first. It uses real commit IDs,
a nested Rust file, arrow-key PR selection, comment popup placement, active-session
submission shortcuts, and zero GitHub content downloads for local reviews.

The optional Kak profile loads its full keybindings after WhichKey, as in the
user's compatible configuration. It does not enable Kak's bundled bridge that
mutates private WhichKey presets. This checks that LGTM's buffer-local review
keys work with Kak mappings while keeping Kak outside the runtime requirements.

Pending-draft editor flows save, edit, reply and delete through real keybindings,
then restart both the editor and backend and submit the same pending review with
a final comment. Separate scenarios cover permission recovery and accepted
comment/review creation with lost responses; recovery must not duplicate writes.

Source-preservation journeys stage exact CRLF bytes, commit them, change line
positions, rename Unicode paths, lose selected text, delete and restore files,
and restart the backend/editor. Separate flows move saved notes between parents
and move ignored-directory roots twice. They assert displayed cards as well as
stable stored identities. Backend/CLI failures retain their isolated runtime
and backend logs; editor failures retain the active scenario and diagnostics.

Export journeys exercise numbered Markdown, old-side locations, replies, lost
locations, and final feedback through backend, CLI, and editor preview/file/clipboard
entry points. Exports must leave the saved conversation unchanged. The editor
also observes external private-review discard and provider-outdated threads,
without recreating remote drafts or attaching historical lines as current.
