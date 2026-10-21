# End-to-end specifications

Backend, CLI and Neovim journeys use TypeScript and Vitest, the Vite test runner. Each suite has its own `mise.toml`. There is no `e2e/mise.toml`. Rust unit and integration tests remain in their apps and libraries.

| Suite | Read first | Public boundary | Run |
| --- | --- | --- | --- |
| Backend | [Local feedback](backend/local-review.spec.ts), [GitHub reviews](backend/github-review.spec.ts) | Real standalone backend and Connect JSON | `premise run e2e:backend` |
| CLI | [Review lifecycle](cli/review.spec.ts) | Real CLI commands and bundled backend | `premise run e2e:cli` |
| Neovim | [Notes](nvim/notes-and-directory.spec.ts), [CodeDiff](nvim/native-review.spec.ts), [anchors](nvim/anchor-lifecycle.spec.ts), [GitHub](nvim/github-review.spec.ts), [editor services](nvim/editor-services.spec.ts) | Real editor, keys, modals, CodeDiff and backend | `premise run e2e:nvim` |

The local suites contain 23 journey runs: six backend, three CLI and seven editor journeys in each of two profiles. Named Given/When/Then steps explain each stage. Dashed Arrange, Act and Assert sections separate setup, actions and checks. Long journeys repeat these sections for each checkpoint; writes are kept outside `expect(...)` so the action is visible. A failed assertion reports the journey and active step. Journeys cover complete lifecycles rather than starting a process for each operation.

## Shared fixtures

```text
e2e/
├── common/
│   ├── runtime.ts          # Processes, isolated environments, Git, CLI and RPC
│   ├── github.ts           # Local GitHub HTTP simulator
│   ├── editor.ts           # Real Neovim MessagePack connection and editor actions
│   ├── live-github.ts      # Existing private repository; run-owned PRs and refs
│   ├── fixtures.ts         # Vitest lifecycle and cleanup
│   ├── config.ts           # Common Vitest settings
│   ├── check-parity.ts     # Historical coverage-map validation
│   └── anchor_cases.json   # Shared source-placement examples
├── backend/                # Specifications, Vitest config, mise.toml
├── cli/                    # Specifications, Vitest config, mise.toml
├── nvim/                   # Specifications, dependency lock, setup, mise.toml
├── PARITY.md               # Historical behavior-to-journey map
├── parity.json             # Machine-readable map
└── tsconfig.json
```

Normal runs use temporary local Git repositories, isolated databases and the local GitHub simulator. They do not use GitHub credentials or create remote repositories. Fixture controls exercise permission denial, rate limits, slow responses, paused mutations, lost responses, commits, renames, deleted selections and external draft removal. The simulator records requests so tests can assert that recovery does not repeat writes.

The editor profiles pin CodeDiff, WhichKey and nvim-notify. The second profile adds Kak with the same compatible key setup used locally; Kak remains optional for users. Both use real native CodeDiff views and an upgraded installed database. Lua contains only runtime setup and commands needed to operate Neovim. TypeScript contains the scenarios, orchestration and assertions.

Failed runs retain their database, workspace, backend log and editor log under `target/e2e/`. Successful runs remove their temporary state. Child processes are stopped in fixture teardown. All waits and RPC requests have deadlines.

## Run and inspect

```sh
premise run install
premise run e2e:list
premise run e2e
premise run e2e:typecheck
premise run e2e:lint
premise run e2e:format:check
premise run e2e:parity
```

Build once with `premise run e2e:nvim`, then use `premise run e2e:nvim:run` while changing editor tests. To run one specification against those existing binaries:

```sh
LGTM_E2E_SPEC=native-review.spec.ts premise run e2e:nvim:run
```

Do not repeat passing suites when their inputs have not changed. Use one complete local gate after changes to shared fixtures.

## Permanent private GitHub fixture

The default is [skapoor8/lgtm-e2e](https://github.com/skapoor8/lgtm-e2e). This is the same existing repository previously used by a smoke test, renamed once for permanent reuse. Repository creation is not part of setup or testing.

The live journey is opt-in and uses local `gh` credentials:

```sh
LGTM_LIVE_GITHUB_E2E=1 premise run e2e:github:live

# Use another existing private fixture repository:
LGTM_E2E_GITHUB_REPOSITORY=owner/private-fixture LGTM_LIVE_GITHUB_E2E=1 premise run e2e:github:live
```

The configured repository must exist, be private, have an initial commit and be active. Missing, public or archived repositories fail before any writes. The runner never creates, recreates, renames, archives or deletes repositories. Do not point it at a production repository.

An atomic `lgtm-e2e-lock` branch serializes live runs across machines. Each run creates uniquely named base/head branches and one PR. Cleanup closes that PR and removes only those branches and the acquired lock. The default branch and repository remain. If a process is killed before cleanup, inspect its logged PR and branches before removing its stale lock; the runner does not steal locks or delete other runs' objects.

The live flow checks actual private pending drafts, edits, replies, pushes, lost-line fallback, self-review validation, final Comment submission, resolve/reopen, deletion and Markdown export. It has a separate Vitest config and stays outside routine local runs and ordinary CI, even if the opt-in environment variable is inherited.

## Coverage history

The legacy Rust, Lua and Python E2E suites have been removed from the checkout after review. [PARITY.md](PARITY.md) preserves the migration map of 76 legacy behaviors to active journey steps. `premise run e2e:parity` checks that those replacement steps still exist. It validates references, not assertion equivalence, and does not substitute for running the tests.

Lua remains in `nvim/init.lua` for editor setup. The plugin’s separate Lua unit tests remain under `apps/lgtm.nvim/tests/`.
