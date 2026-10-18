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
