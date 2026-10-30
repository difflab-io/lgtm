# End-to-end specifications

Backend, CLI and Neovim journeys use TypeScript and Vitest. Each suite has its own `mise.toml`. There is no `e2e/mise.toml`.

| Suite | Features | Public boundary | Run |
| --- | --- | --- | --- |
| Backend | [Notes](backend/notes.spec.ts), [local reviews](backend/local-review.spec.ts), [PR reviews](backend/pr-review.spec.ts), [diff data](backend/diffs.spec.ts) | Standalone backend and Connect JSON | `premise run e2e:backend` |
| CLI | [Notes](cli/notes.spec.ts), [local reviews](cli/local-review.spec.ts), [PR reviews](cli/pr-review.spec.ts) | CLI commands and bundled backend | `premise run e2e:cli` |
| Neovim | [Plugin shell](nvim/editor.spec.ts) | Real editor startup, Lua, Ex commands, keys, diagnostics and package help | `premise run e2e:nvim` |

Native Vitest `describe` groups name states, such as `When resuming an existing draft review`. Each `it` names one visible behavior. Suite and feature names come from Vitest tags. Plain `// Arrange`, `// Act` and `// Assert` comments separate setup, actions and assertions. Tests use Vitest directly, without custom test aliases or step runners.

Live and recording runs share one PR per state group through `beforeAll` and `afterAll`; replay loads an independent recording for each case. Each case gets a fresh database. Cleanup removes its feedback. It restores the branch only if its head changed and reopens the PR only if it closed. With one reviewer, unchanged PRs use four housekeeping reads per case, plus feedback deletions. A merged PR requires another PR because GitHub cannot reopen it. GitHub retains submitted review records and their final messages. Cases identify their own feedback instead of relying on array order or counting all review records.

## Test boundaries

PR E2E replays genuine GitHub HTTP recordings by default. Set `LGTM_E2E_GITHUB_MODE=live` to run the same journeys and assertions against real GitHub without changing recordings, or use the `e2e:record` task to refresh them. Unknown modes and missing recordings fail; replay never falls back to the network. REST and GraphQL responses come from recorded GitHub interactions, with strict request matching and ordered writes. Reads may poll, reorder or reuse past recorded observations when asynchronous requests overlap; responses from after a future write remain unavailable until that write matches. Locally generated UUIDs are bound consistently across requests and responses; other request fields must match. LGTM processes, databases, CLI commands, editor actions and Git remain real. The fixture never intercepts LGTM RPCs or simulates GitHub review behavior. Notes and local reviews use temporary local files, Git repositories and real databases. Each suite checks its own public boundary.

```text
e2e/
├── common/
│   ├── test-harness.ts            # Processes, isolated environments, Git, CLI and RPC
│   ├── github.ts                  # Shared run-owned PR/ref lifecycle
│   ├── github-driver.ts           # Provider contract and real GitHub transport
│   ├── github-recording.ts        # Loopback record/replay transport and captured Git objects
│   ├── github-cassette.ts         # Request matching, UUID binding and mutation barriers
│   ├── github-cassette.test.ts    # Replay infrastructure contracts
│   ├── fixtures.ts                # Local process lifecycle
│   ├── pr.fixtures.ts             # PR group lifecycle, composed with the test harness fixture
│   ├── test-utils.ts              # Polling, content hashes and wire-value types
│   ├── config.ts
│   └── anchor_cases.json
├── backend/
│   ├── notes.spec.ts
│   ├── local-review.spec.ts
│   ├── pr-review.spec.ts
│   ├── diffs.spec.ts
│   ├── identity.spec.ts
│   ├── anchors.ts
│   ├── local-review.fixtures.ts
│   ├── pr-review.fixtures.ts
│   ├── vitest.config.ts
│   └── mise.toml
├── cli/
│   ├── notes.spec.ts
│   ├── local-review.spec.ts
│   ├── pr-review.spec.ts
│   ├── vitest.config.ts
│   └── mise.toml
├── nvim/
│   ├── editor.spec.ts
│   ├── editor.ts
│   ├── fixtures.ts
│   ├── pr-review.fixtures.ts
│   ├── profiles.ts
│   ├── init.lua
│   ├── dependencies.ts
│   ├── install.ts
│   ├── dependencies.lock.json
│   ├── vitest.config.ts
│   └── mise.toml
└── tsconfig.json
```

The required editor profile loads the plugin shell with pinned CodeDiff, WhichKey and nvim-notify dependencies retained for the rewrite. The optional `kak` profile adds pinned Kak keybindings. Runs use only the required profile by default. Set `LGTM_E2E_NVIM_PROFILES=required,kak` to include Kak, or `kak` to run it alone. Lua contains editor runtime setup. TypeScript owns scenarios, command execution, input, assertions and process lifecycle. The shared runtime still starts a real CLI/backend with a fresh database, while the plugin shell makes no backend calls.

`common/` owns processes, credentials, GitHub resources and test configuration. Backend fixtures construct API states. Neovim fixtures perform editor actions. Large feature setups live beside their spec as `*.fixtures.ts`. Small setups used by one file stay private in that file. Tests keep the actions and assertions that describe the behavior under test.

Fixture hooks compose in stack order. Setup creates the PR, starts the backend, then opens the editor. Cleanup closes the editor, stops the backend, resets the PR, then removes the group’s refs. The local and PR editor fixtures share the same editor lifecycle.

Failed runs retain their workspace, database and logs under `target/e2e/`. Editor failure logs also include the rendered screen and window viewports to diagnose editor failures from CI output. Successful runs remove temporary local state. Fixture hooks stop processes after each case. Waits, subprocesses and RPC requests have deadlines.

## Run and inspect

```sh
premise run install
premise run e2e:list
premise run e2e
premise run e2e:typecheck
premise run e2e:lint
premise run e2e:format:check
```

The `e2e` task builds the binaries before it runs the suites. After a build, use the `:run` tasks for focused tests. Select a feature file and profile with native Vitest arguments:

```sh
premise run e2e:nvim:run -- editor.spec.ts --tags-filter=required
premise run e2e:backend:run -- pr-review.spec.ts -t "When resuming"
```

For a quick local check, run the existing scenarios tagged `smoke`:

```sh
premise run e2e:smoke
# Or run one suite, with native Vitest filters when needed:
premise run //e2e/nvim:smoke
premise run e2e:nvim:run -- --tags-filter=smoke
```

The default smoke selection has nine cases: four backend, three CLI and two Neovim shell cases. Backend and CLI cases cover notes, local reviews, diff data and remote drafts. Neovim smoke checks shell startup and Lua, Ex command and key control. Editor smoke uses the required profile by default. These are the same tests used by the full suite. PR cases use the selected GitHub provider and clean up their run-owned PRs and refs. The smoke tasks build changed binaries and install selected editor dependencies; the `:run` example uses existing binaries.

Run focused tests during development. Use a file, test name or tag to select cases. Use `e2e:smoke` for a quick check across features. Run all tests for final validation or changes to shared fixtures. Reuse passing results for unchanged code.

CI runs the root `e2e` task on Linux and macOS. The suites share setup and run in parallel, alongside the replay infrastructure contracts. A failed suite does not stop the other suites or their cleanup.

Normal CI uses replay on both operating systems and does not receive GitHub fixture credentials. On Commit runs when a PR targeting `main` opens, reopens or receives new commits; branch pushes without an open PR do not run it. New commits cancel older runs for the same PR. On Merge handles pushes to `main`. Run parity locally with `LGTM_E2E_GITHUB_MODE=live`, using the same suite tasks, spec files and assertions as normal CI. There is no quota gate.

## Extended tests

Default runs omit tests tagged `extended` and use only the required editor profile. Add `--extended` to include extended tests and both editor profiles:

```sh
premise run e2e
premise run e2e -- --extended
premise run e2e:backend -- --extended
```

The root `e2e` task runs the suites in parallel and shares their build dependencies. Each suite keeps its tests sequential. The `extended` tag covers repeated backend and CLI review journeys. An explicit `LGTM_E2E_NVIM_PROFILES` value overrides the profile choice.

## Record, replay and live validation

```sh
# Default: replay, with no GitHub credentials or fixture API traffic:
premise run e2e
# Run unchanged journeys against real GitHub without updating recordings:
LGTM_E2E_GITHUB_MODE=live premise run e2e:smoke
LGTM_E2E_GITHUB_MODE=live premise run e2e
# Refresh JSON for one selected PR journey (backend or cli):
premise run e2e:record -- backend pr-review.spec.ts -t "User can immediately persist a file-level draft comment"
# Refresh every journey, including extended cases, using only the required editor profile:
premise run e2e:record -- all
# Verify replay matching and mutation barriers without GitHub:
premise run e2e:fixtures:test
```

Recordings live under `e2e/recordings/<suite>/<spec>.json`, with one entry per journey keyed by `<test>-<id>`. Each entry contains the fixture metadata, deduplicated HTTP responses and the requests that refer to them. Successful selected cases replace only their own entries and preserve sibling journeys; failed cases leave existing recordings intact. Entries are sorted by key for stable diffs. Record mode forwards actual requests to GitHub and saves the responses. Authorization headers and authenticated-user email/plan metadata are excluded, and saving a known credential in JSON is rejected. Review the JSON diff before committing it. A renamed journey needs an entry under its new name; remove obsolete entries when removing tests.

Historical Neovim recordings remain available for future fixture work; the shell tests do not use GitHub recordings. The retained PR editor fixture can compose the shared GitHub fixture with editor startup when replacement PR journeys are added.

The refresh task builds the selected suite and accepts its usual Vitest filters. `all` accepts no filters and runs suites serially to avoid API bursts. Recording forwards mutating API requests at least one second apart. Live and recording setup use Git pushes for run-owned source branches rather than GitHub content-creation APIs. Both validate private visibility before writes and clean up only run-owned PRs and refs. A local replay pass cannot establish current GitHub parity; use a focused live run or refresh the affected journeys when changing provider behavior.

## Permanent private GitHub fixture (live mode)

The default is [skapoor8/lgtm-e2e](https://github.com/skapoor8/lgtm-e2e). Tests never create, recreate, rename, archive or delete repositories. The fixture must already exist, be private, have an initial commit and be active. Invalid fixtures fail before writes.

Locally, log in to both GitHub accounts with `gh auth login`. The fixture reads each stored token by account name. It never switches your active account. The primary account creates the PR. The second account acts as another reviewer and needs write access to the same private repository.

The primary account defaults to `skapoor8`. Override it with `LGTM_E2E_GITHUB_PRIMARY_USER`. The second account defaults to `skapoor81`. Override it with `LGTM_E2E_GITHUB_SECONDARY_USER`. The test processes read the primary token from `LGTM_E2E_GITHUB_TOKEN` and the second token from `LGTM_E2E_GITHUB_SECONDARY_TOKEN`.

A repository-scoped primary token needs Contents, Issues and Pull requests read/write, plus Metadata read access. Tests need no repository administration permission. Keep tokens in local credential storage.

GitHub does not support fine-grained tokens for a collaborator on another personal account’s repository. The collaborator account `skapoor81` therefore needs a classic token with the broader `repo` scope for this fixture. See [GitHub’s token limitations](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#fine-grained-personal-access-tokens-limitations). Local runs use the existing account logins and do not need new tokens.

```sh
# Use another existing private fixture repository:
LGTM_E2E_GITHUB_MODE=live LGTM_E2E_GITHUB_REPOSITORY=owner/private-fixture premise run e2e
```

Each PR state group creates unique base/head branches and one PR in that same repository. Cleanup closes its PR and removes only its own branches. Parallel runs use separate refs; no global lock branch is needed. The default branch remains unchanged. If a process is killed, use its logged PR and run identifier to inspect and clean up that run's refs.

Premise removes generic publication credentials such as `GH_TOKEN` from task environments. Set `LGTM_E2E_GITHUB_TOKEN` and `LGTM_E2E_GITHUB_SECONDARY_TOKEN` to supply tokens explicitly, or use the stored account logins described above. `LGTM_E2E_GITHUB_REPOSITORY` selects a different permanent private fixture. The PR fixtures pass the selected token only to their test processes and GitHub subprocesses. Ordinary commit and PR jobs receive no fixture credential.


Private Rust unit tests, database integration tests and the plugin's Lua load checks remain in their existing app and library locations.

Neovim runs a global preflight before fixture setup. It starts each selected profile, verifies shell loading and startup errors, and computes a native CodeDiff diff. Missing modules fail the suite once, with the startup error. The installer fetches pinned CodeDiff, WhichKey and nvim-notify dependencies, plus Kak when selected. It always checks out the selected revision, including on a fresh clone whose default HEAD already matches the pin.
