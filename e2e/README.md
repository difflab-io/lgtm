# End-to-end specifications

Backend, CLI and Neovim journeys use TypeScript and Vitest. Each suite has its own `mise.toml`. There is no `e2e/mise.toml`.

| Suite | Features | Public boundary | Run |
| --- | --- | --- | --- |
| Backend | [Notes](backend/notes.spec.ts), [local reviews](backend/local-review.spec.ts), [PR reviews](backend/pr-review.spec.ts), [diff data](backend/diffs.spec.ts) | Standalone backend and Connect JSON | `premise run e2e:backend` |
| CLI | [Notes](cli/notes.spec.ts), [local reviews](cli/local-review.spec.ts), [PR reviews](cli/pr-review.spec.ts) | CLI commands and bundled backend | `premise run e2e:cli` |
| Neovim | [Notes](nvim/notes.spec.ts), [local reviews](nvim/local-review.spec.ts), [PR reviews](nvim/pr-review.spec.ts), [CodeDiff](nvim/diffs.spec.ts), [editor services](nvim/editor.spec.ts) | Editor, keys, modals, CodeDiff and backend | `premise run e2e:nvim` |

Native Vitest `describe` groups name states, such as `When resuming an existing draft review`. Each `it` names one visible behavior. Suite and feature names come from Vitest tags. Plain `// Arrange`, `// Act` and `// Assert` comments separate setup, actions and assertions. Tests use Vitest directly, without custom test aliases or step runners.

Each PR state group shares one PR through `beforeAll` and `afterAll`. Each case gets a fresh database. Cleanup removes its draft feedback and restores the original head commit between cases. A merged PR requires another PR because GitHub cannot reopen it. GitHub retains submitted review records and their final messages. Cases identify their own feedback instead of relying on array order or counting all review records.

## Test boundaries

PR E2E uses real GitHub. There is no simulated provider option, opt-in live mode or separate live spec file. Notes and local reviews use temporary local files, Git repositories and real databases. Each suite checks its own public boundary; backend RPCs, CLI commands and editor actions are different contracts.

```text
e2e/
├── common/
│   ├── test-harness.ts            # Processes, isolated environments, Git, CLI and RPC
│   ├── github.ts                  # Existing private repository; run-owned PRs and refs
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
│   ├── notes.spec.ts
│   ├── local-review.spec.ts
│   ├── pr-review.spec.ts
│   ├── diffs.spec.ts
│   ├── editor.spec.ts
│   ├── assertions.ts
│   ├── editor.ts
│   ├── fixtures.ts
│   ├── local-review.fixtures.ts
│   ├── pr-review.fixtures.ts
│   ├── init.lua
│   ├── install.ts
│   ├── dependencies.lock.json
│   ├── vitest.config.ts
│   └── mise.toml
└── tsconfig.json
```

The editor profiles pin CodeDiff, WhichKey and nvim-notify. The second profile adds Kak with compatible keys; Kak remains optional for users. Both exercise native CodeDiff views against a fresh database created by the current backend. Lua contains editor runtime setup. TypeScript owns the scenarios, input, assertions and process lifecycle.

`common/` owns processes, credentials, GitHub resources and test configuration. Backend fixtures construct API states. Neovim fixtures perform editor actions. Large feature setups live beside their spec as `*.fixtures.ts`. Small setups used by one file stay private in that file. Tests keep the actions and assertions that describe the behavior under test.

Fixture hooks compose in stack order. Setup creates the PR, starts the backend, then opens the editor. Cleanup closes the editor, stops the backend, resets the PR, then removes the group’s refs. The local and PR editor fixtures share the same editor lifecycle.

Failed runs retain their workspace, database and logs under `target/e2e/`. Successful runs remove temporary local state. Fixture hooks stop processes after each case. Waits, subprocesses and RPC requests have deadlines.

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
premise run e2e:nvim:run -- diffs.spec.ts --tags-filter=required
premise run e2e:backend:run -- pr-review.spec.ts -t "When resuming"
```

For a quick local check, run the existing scenarios tagged `smoke`:

```sh
premise run e2e:smoke
# Or run one suite, with native Vitest filters when needed:
premise run //e2e/nvim:smoke
premise run e2e:nvim:run -- --tags-filter=smoke
```

The smoke selection has 11 cases: four backend, three CLI and four Neovim cases. It covers notes, local reviews, diff data and native CodeDiff, plus remote draft creation through each public boundary. The editor smoke cases use the required dependency profile; the full suite also covers Kak. These are the same tests used by the full suite. PR cases use real GitHub and clean up their run-owned PRs and refs. The smoke tasks build changed binaries and install missing editor dependencies; the `:run` example uses existing binaries.

During development, run only the tests relevant to the changed behavior. Use a spec path, test name or tag for focused checks. Use smoke when a quick check across features is useful. Reserve the full suite for final validation or changes to shared fixtures or behavior that require all suites. Do not repeat passing suites whose inputs have not changed. CI continues to run the full suites.

## Permanent private GitHub fixture

The default is [skapoor8/lgtm-e2e](https://github.com/skapoor8/lgtm-e2e). Tests never create, recreate, rename, archive or delete repositories. The fixture must already exist, be private, have an initial commit and be active. Invalid fixtures fail before writes.

Locally, log in to both GitHub accounts with `gh auth login`. The fixture reads each stored token by account name. It never switches your active account. The primary account creates the PR. The second account acts as another reviewer and needs write access to the same private repository.

The primary account defaults to `skapoor8`. Override it with `LGTM_E2E_GITHUB_PRIMARY_USER`. The second account defaults to `skapoor81`. Override it with `LGTM_E2E_GITHUB_SECONDARY_USER`. For CI, supply the primary token through `GH_TOKEN` and the second token through `LGTM_E2E_GITHUB_SECONDARY_TOKEN`.

A repository-scoped primary token needs Contents, Issues and Pull requests read/write, plus Metadata read access. Tests need no repository administration permission. Keep tokens in local credential storage or CI secrets.

GitHub does not support fine-grained tokens for a collaborator on another personal account’s repository. The second CI account therefore needs a classic token with the broader `repo` scope for this fixture. See [GitHub’s token limitations](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#fine-grained-personal-access-tokens-limitations). Local runs use the existing account logins and do not need new tokens.

```sh
# Use another existing private fixture repository:
LGTM_E2E_GITHUB_REPOSITORY=owner/private-fixture premise run e2e
```

Each PR state group creates unique base/head branches and one PR in that same repository. Cleanup closes its PR and removes only its own branches. Parallel runs use separate refs; no global lock branch is needed. The default branch remains unchanged. If a process is killed, use its logged PR and run identifier to inspect and clean up that run's refs.

CI reads the repository secrets `LGTM_E2E_GITHUB_TOKEN` and `LGTM_E2E_GITHUB_SECONDARY_TOKEN`. Both are required for cross-account review cases. The optional Actions variable `LGTM_E2E_GITHUB_REPOSITORY` selects a different permanent private fixture. The ordinary workflow `GITHUB_TOKEN` cannot write to another private repository. Real PR E2E runs on trusted branches; fork PRs do not receive the fixture credential. Keep the secret renewed before its expiry.


Private Rust unit tests, database integration tests and the plugin's Lua unit contracts remain in their existing app and library locations.
