# End-to-end specifications

The active TypeScript/Vitest suites check the hello-review skeleton, CLI command shape and Neovim shell; each suite keeps its own Mise config, and there is no root `e2e/mise.toml`.

## Test boundaries

| Suite | Behavior | Command |
| --- | --- | --- |
| Forge | Real hello-review RPC, name normalization, removed RPC rejection and existing DB file preservation | `premise run e2e:forge` |
| CLI | Command parsing, local snapshot review lifecycle (`--local`), remote GitHub PR review lifecycle, local/remote diff output parity, UI/daemon serving and shutdown | `premise run e2e:cli` |
| Neovim | Shell loading, Lua/Ex/key control, diagnostics and package-manager setup | `premise run e2e:nvim` |
| Replay infrastructure | Strict HTTP snapshots, UUID binding, ordered mutation barriers, recording safety and retained Git reconstruction | `premise run e2e:fixtures:test` |

All default checks run without GitHub credentials or remote forge writes; the dummy DB package opens no SQL database.



The old notes, local-review, PR-review, identity and diff journeys have been removed; their JSON recordings remain inactive historical fixtures under `e2e/recordings/`, not evidence of supported application behavior.

```text
e2e/
├── common/
│   ├── github-cassette.ts         # Strict matching, UUID binding, mutation barriers
│   ├── github-cassette.test.ts    # Offline mechanism checks
│   ├── github-recording.ts        # HTTP recording/replay proxy and captured Git objects
│   ├── github-driver.ts           # Provider transport and process-scoped credentials
│   ├── github.ts                 # Run-owned PR/ref lifecycle
│   ├── pr.fixtures.ts            # Reusable future provider journey setup
│   ├── record.ts                 # Guard until recordable journeys return
│   └── test-harness.ts            # Real application processes, RPC, CLI and Git
├── forge/hello-review.spec.ts
├── cli/
│   ├── commands.spec.ts
│   ├── daemon.spec.ts
│   ├── local-review.fixtures.ts
│   ├── local-review.spec.ts
│   ├── parity.spec.ts
│   ├── pr-review.fixtures.ts
│   └── pr-review.spec.ts
├── nvim/                         # Retained editor setup, profiles, pins and shell tests
└── recordings/                   # Recorded cassettes, including captured Git bundles
```

`github-cassette.ts` keeps exact request matching and consistent runtime UUID bindings; reads may reuse past observations, while future responses remain unavailable until the corresponding recorded write matches.

Unrecorded requests, duplicate writes and omitted recorded writes fail rather than falling back to the network.

Each cassette stores versioned fixture metadata, deduplicated HTTP responses, ordered exchanges and optional captured Git objects; the mechanism test reconstructs a Git repository from an existing Neovim cassette without application RPCs.

The recorder replaces only successful selected journeys, preserves siblings and failed-run snapshots, strips authorization/private user metadata and rejects known credentials in saved JSON.

Offline tests exercise the recording proxy with a synthetic upstream response and replay the saved result, so default validation does not contact GitHub.

## Run and inspect

```sh
premise run e2e:smoke
premise run e2e
premise run e2e:typecheck
premise run e2e:lint
premise run e2e:format:check
```

Suite tasks build their required binaries; after building, use the `:run` tasks for narrower checks:

```sh
premise run e2e:forge:run -- hello-review.spec.ts
premise run e2e:cli:run -- commands.spec.ts
premise run e2e:nvim:run -- editor.spec.ts --tags-filter=required
```

Run focused cases during development and reserve the full suite for final validation or changes to shared fixtures; successful unchanged checks can be reused.

Native Vitest `describe` groups identify `When ...` states, and `it` cases assert observable results with Arrange/Act/Assert sections; shared lifecycle hooks own application processes and temporary workspaces under `target/e2e/`.

Successful runs remove temporary state; failed runs retain workspaces and process logs, and editor failures also include screen/window diagnostics. Each CLI E2E case also writes its full `lgtm` command transcript (`$ lgtm ...`, exit code, `stdout`, and `stderr`) to `e2e/outputs/<suite>/<spec>/<test>.log` (git-ignored) for inspection after both passing and failing runs.

The CLI safety case uses a temporary checkout and bare upstream to verify that unimplemented commands leave file bytes, index, HEAD, refs and upstream refs unchanged.

## Extended tests

The required profile retains pinned CodeDiff, WhichKey, nvim-notify and NuiComponents/NUI dependencies, plus lazy.nvim and Plenary for package-manager checks; optional `kak` adds pinned Kak keybindings.

```sh
LGTM_E2E_NVIM_PROFILES=required,kak premise run e2e:nvim
```

`premise run e2e -- --extended` includes both profiles unless `LGTM_E2E_NVIM_PROFILES` explicitly overrides the selection.

Editor preflight checks dependency loading and a native CodeDiff operation; the plugin remains a shell without review commands or feature windows, and its development/release source-switch installation tasks remain available.

## Record, replay and live validation

The `e2e:record` entry point runs Vitest with `LGTM_E2E_GITHUB_MODE=record` for active provider suites (`cli`, `forge` or `all`):

```sh
premise run e2e:record -- cli
```

When replacement provider journeys are added, compose `common/pr.fixtures.ts` with the shared runtime and re-enable this entry point for eligible suites; do not repurpose historical cassettes as hello-review fixtures.

The retained provider fixture supports `LGTM_E2E_GITHUB_MODE=replay|record|live`: replay is strict and offline, record refreshes selected HTTP snapshots after successful real interactions, and live verifies current provider parity without refreshing snapshots.

Recording files live at `e2e/recordings/<suite>/<spec>.json`, keyed by journey name and stable title hash; review redaction and JSON diffs before committing a refreshed recording.

A replay pass does not prove live GitHub parity, and old snapshots should only be refreshed after replacement provider behavior and assertions exist.

## Permanent private GitHub fixture (live mode)

Live/record mode must use the permanent private repository selected by `LGTM_E2E_GITHUB_REPOSITORY` (default `skapoor8/lgtm-e2e`), verify private visibility before writes and clean up only run-owned PRs and refs; never create, recreate, rename, archive or delete a fixture repository.

Retained account selection uses `LGTM_E2E_GITHUB_PRIMARY_USER`/`LGTM_E2E_GITHUB_SECONDARY_USER` and process-scoped `LGTM_E2E_GITHUB_TOKEN`/`LGTM_E2E_GITHUB_SECONDARY_TOKEN`, or stored per-account `gh` tokens, without switching the user's active account.
