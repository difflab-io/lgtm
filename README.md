---
base_commit: '386919a863bed440d232e70e3aeb29937781c57c'
---

# PoC: standalone web and Neovim reviews

Experimental branch only. Freeze as a tag; do not merge or release this branch.

## Purpose

Can one local Go gRPC service power a CLI, SvelteKit web app, and Neovim plugin for worktree and GitHub reviews, with distinct human and agent authors?

The success criterion is a runnable flow in both UIs: create a draft, add comments, promote standalone notes, save a summary, and submit a comment, approval, or request for changes. GitHub submissions must preserve author attribution and reject a changed PR head.

## Setup and tasks

Requires mise, Go 1.25.14, Node 26.10.0, Git, and Neovim 0.11+ with curl. `mise install` selects project tools. Run `npm ci` to install web dependencies. GitHub operations use `gh api` and the server user's `gh auth login` credentials; install the GitHub CLI for remote reviews. No GitHub token is stored in review state.

Run from the repository root:

```sh
mise run build
mise run test
mise run run:web
```

Open http://127.0.0.1:9910. The CLI starts the service on 127.0.0.1:9911 and serves the SvelteKit static build. `bin/lgtm serve --repo /path/to/checkout --ui apps/lgtm-web-app/build` reviews another checkout. SQLite defaults to that checkout's `.lgtm/lgtm.db`. Use `--db /path/to/lgtm.db` to override it. Multiple processes share database transactions. `run:server` is the separate Go server entrypoint and uses the same ports, so run it instead of `run:web`.

| Task | Purpose | Observed result |
| --- | --- | --- |
| `mise run build` | Build SvelteKit and both Go executables | Passed |
| `mise run test` | Race-enabled Go tests, Svelte/TypeScript checks, plugin load check | Passed |
| `mise run run:web` | Start CLI-hosted service and browser UI | Passed: startup and shutdown checked |
| `mise run run:server` | Start standalone service and HTTP gateway | Passed: startup and shutdown checked |
| `mise run run:nvim` | Headless plugin command registration example | Passed |
| `mise run run:smoke` | Ephemeral service and real gRPC review creation | Passed |
| `mise run test:browser` | Browser workflow against running service | Passed |
| `mise run test:nvim` | Neovim workflow against running service | Passed |

Browser checks need Chromium (`npx playwright install chromium`) or `LGTM_CHROME=/path/to/chrome mise run test:browser`. Both client integration checks start isolated services on available ports. The Neovim integration check uses your normal configuration with CodeDiff installed.

## Database and protobuf backend

SQLite is the default database. It stores workspace records, reviews, comments, replies, and import history in separate tables. Foreign keys keep comments attached to valid reviews and threads. WAL mode supports database recovery. A database transaction applies each mutation in full or rolls it back. Mutations reload the latest state after acquiring a database lock, so separate server processes do not overwrite each other's work.

The storage interface in `internal/storage` supports SQLite and PostgreSQL. Embedded SQL migrations create the same entity schema for both drivers. Migration checksums prevent silent edits to an applied migration. Run the same server with a PostgreSQL connection for a hosted deployment:

```sh
# Set LGTM_DATABASE_URL to your PostgreSQL connection string.
mise run run:postgres
# Or mount a specific checkout and assign a stable workspace ID:
bin/lgtm-server --repo /srv/checkouts/premise \
  --database postgres --workspace-id cloudvoyant/premise
```

The server reads PostgreSQL credentials from `LGTM_DATABASE_URL`. `--db` also accepts a PostgreSQL connection string. Set `--workspace-id` consistently across replicas that serve the same mounted repository. Different workspace IDs isolate their review data in the same database. The default workspace ID is the canonical local checkout path. Selecting PostgreSQL does not transfer existing SQLite data automatically.

The server serves one configured checkout per process. The default listeners remain on loopback. Hosted authentication, TLS termination, and repository provisioning belong to the deployment layer. The database switch does not provide those controls.

Existing `.lgtm/state.json` files import once when the default SQLite database starts. The original JSON remains intact. An explicit database path does not automatically import checkout state. For an explicit migration, use:

```sh
bin/lgtm migrate --repo /path/to/checkout \
  --from /path/to/checkout/.lgtm/state.json \
  --db /path/to/checkout/.lgtm/lgtm.db
```

Import preserves IDs, agent identities, draft status, thread replies, and resolution state. Repeating the same import has no effect. Import refuses changed source JSON or a nonempty target without matching import history. Stop the old JSON server before switching backends.

The protobuf schema lives in `proto/lgtm/v1/review.proto`. The gRPC service has 13 named methods with generated request and response types. Standard gRPC health checks and server reflection are enabled. `/healthz` checks the database connection. Requests have deadlines, and shutdown drains in-flight work before closing the database.

```sh
mise run proto:generate
mise run proto:check
mise run test:postgres
```

Generation installs pinned tools into the ignored `.tmp/proto-tools` directory. `test:postgres` starts an isolated PostgreSQL cluster and removes it after testing. It requires PostgreSQL binaries on PATH or `LGTM_PG_BIN`. On this machine, it also finds Homebrew's PostgreSQL 17 installation. Set `LGTM_TEST_POSTGRES_URL` to test against an existing disposable database instead. Tests create their own workspace IDs.

## Try a review with sample repositories

The PoC includes a fixture script that clones three open-source repositories into the git-ignored `.tmp` directory. Each checkout is pinned to a release commit so recreating the fixtures produces the same changes.

```sh
mise run fixtures:setup
mise run fixtures:status
mise run fixtures:web:go
```

The last command builds LGTM and opens its service against `.tmp/go`. Visit http://127.0.0.1:9910, create a **Local worktree** draft, and inspect the diff. Use `fixtures:web:rust` or `fixtures:web:ts` to try the other languages. Stop the running service before switching fixtures; all three launch tasks use the same ports.

| Checkout | Upstream release | Source file to open | Intentional review issue |
| --- | --- | --- | --- |
| `.tmp/rust` | [rust-lang/regex](https://github.com/rust-lang/regex/tree/1.11.1), `1.11.1` | `src/regex/string.rs`, line 205 | `is_match` returns the opposite result |
| `.tmp/go` | [go-chi/chi](https://github.com/go-chi/chi/tree/v5.2.1), `v5.2.1` | `middleware/timeout.go`, line 39 | Timeout responses change from 504 to 503 |
| `.tmp/ts` | [sindresorhus/type-fest](https://github.com/sindresorhus/type-fest/tree/v4.41.0), `v4.41.0` | `source/is-any.d.ts`, line 33 | `IsAny` checks for `never` instead of `any` |

Each fixture has a staged source change, an unstaged README change, and an untracked `lgtm-notes.md` file. The changed-file list includes all three. Select a file and use its line buttons to comment. Save a standalone note, then promote it into your draft. Change the author to Codex or Claude with kind **Agent / LLM** to check that the UI distinguishes agent notes from human notes. These deliberately faulty changes are review material; the fixtures are not expected to pass their upstream test suites. No upstream PR is created.

For Neovim, enter a fixture checkout with `cd .tmp/go` and run `nvim .`. Type `:lgtm` to select a local review or an open GitHub PR. The plugin starts its service automatically. Both UIs share the checkout's `.lgtm/lgtm.db`.

Stop the fixture service, then start over with:

```sh
mise run fixtures:recreate
```

This rebuilds the three managed checkouts from their pinned local Git objects, reapplies the edits, and clears their saved test reviews and ignored build outputs. Recreation works offline after initial setup and prepares replacement checkouts before swapping them in. `fixtures:setup` instead keeps existing fixtures, edits, and reviews. Recreation refuses additional source edits, changed staging, new untracked files, changed commits, or changed remotes. If you want to keep that work and start fresh, stop the service, move `.tmp` to a backup directory, then run `fixtures:setup`. Other files at the top level of `.tmp` are left in place. The pinned SHAs and generated-change fingerprints are recorded in `.tmp/lgtm-fixtures.json`; the script lives in `scripts/review-fixtures.mjs`.

Fixture validation passed for initial setup, deterministic offline recreation, clearing saved test reviews, and preserving additional edits by refusing recreation. Each language launch task was checked against the running service: the worktree diff, edited source file, and untracked notes were readable. `.tmp` was verified as git ignored.

## Try your project repositories

The example tasks clone `cloudvoyant/premise`, `cloudvoyant/premise-cargo`, and `cloudvoyant/readership` into `.tmp`. They add a README edit and a new note file for local review.

```sh
mise run examples:setup
mise run examples:web:premise
# Or: mise run examples:web:premise-cargo
# Or: mise run examples:web:readership
```

Open http://127.0.0.1:9910. Each task selects its checkout automatically. Choose a local worktree review or one of its open GitHub PRs. Stop the service before switching tasks.

For Neovim, enter the repository and use your normal configuration:

```sh
cd .tmp/premise
nvim .
# Type :lgtm
```

The same instructions work in `.tmp/premise-cargo`, `.tmp/readership`, or your existing checkouts. The current Git root supplies the repository context. A GitHub origin and authenticated `gh` enable the open PR list. Local reviews remain available if GitHub is unavailable.

Use `mise run examples:recreate` to reset the generated edits and saved reviews. The script refuses additional edits or changed repository revisions. The initial commit and generated-change fingerprints appear in `.tmp/project-examples.json`.

## Neovim

The plugin uses CodeDiff for the file explorer and side-by-side diffs. Full comment cards appear below their source line, with multiline replies and distinct human or agent authors. The opposite pane receives padding to keep the diff aligned. A thread panel and multiline editor support the same comments. Your normal LazyVim configuration now loads this checkout through `~/.config/nvim/lua/plugins/lgtm.lua`.

For another configuration, add this Lazy plugin specification. Replace the path with your checkout path:

```lua
return {
  {
    dir = '/absolute/path/to/lgtm/libs/lgtm-nvim',
    name = 'lgtm.nvim',
    dependencies = { 'esmuellert/codediff.nvim' },
    lazy = false,
    opts = {},
    config = function(_, opts) require('lgtm').setup(opts) end,
  },
}
```

Run the normal Neovim configuration from a Git checkout:

```sh
cd .tmp/go
nvim .
```

Type `:lgtm` to open the Neovim review picker. Neovim requires uppercase user commands. A command-line abbreviation provides the lowercase spelling. Select a local worktree draft, an open GitHub PR, standalone notes, or a saved review. The plugin finds the Git root and starts a service on available ports. GitHub selection lists open PRs from the checkout's `origin` repository. No repository name or PR number is required.

In the CodeDiff pane, press `c` to comment at the cursor or `n` to save a standalone note. Comments preserve the left or right side and actual source line. Write multiline text, then save with `Ctrl-S` or `:w`. On a commented source line or in the thread panel, press `r` to reply, `x` to resolve, and `p` to promote a note. Reopen resolved threads from the thread panel. Resolution stays local. Press `s` to edit the summary, `a` to approve, or `R` to request changes.

Use `:LgtmIdentity` to change the author. `:LgtmComment` and `:LgtmNote` also work in ordinary checkout buffers. `:LgtmThreads` opens the panel. `:LgtmSubmit`, `:LgtmApprove`, and `:LgtmReject` submit the selected draft. Submission opens a summary editor, then asks for confirmation. `:LgtmSubmit` offers comment, approve, and request-changes actions. Failed submission leaves the draft and editor intact for correction or retry. `:LgtmRefresh` reloads shared state. `:LgtmHistory` reads existing GitHub discussion.

The web app starts with a local/PR selection screen. Its review screen follows `poc/gpui-rpc-sqlite`: changed files, a highlighted per-file diff with inline threads, and a discussion sidebar. Use the File tab or the All files filter to comment outside the diff. The sidebar controls identity, replies, resolution, and note promotion. Finish review opens the summary and submission controls.

## Agent example

Agents use the same service as people and supply their identity explicitly:

```sh
printf '%s\n' '{"op":"comment","path":"README.md","line":1,"body":"Explain the setup requirements.","actor":{"name":"Codex","kind":"agent"}}' | bin/lgtm rpc
```

Standalone comments do not sync until promoted and submitted. Use `kind: human` for user comments. The UIs display both name and kind. Agent review summaries and comments receive `> Generated with <llm-name>.` as an appended line when sent to GitHub; human content remains unchanged. GitHub still records the authenticated account as its author. This prototype tracks supplied identities; it does not authenticate agents or invoke models itself.

## Code map

- `apps/lgtm-cli`: `serve`, protobuf gRPC `rpc`, and isolated `smoke` entrypoints.
- `apps/lgtm-server`: independent service entrypoint. Uses the consistent `lgtm` spelling.
- `apps/lgtm-ts-client`: typed HTTP gateway client and review model.
- `apps/lgtm-web-app`: static SvelteKit workspace, source/diff view, discussions, and review controls.
- `libs/lgtm-nvim`: asynchronous Lua plugin. Neovim's `vim.system` runs curl against the gateway, so a separate Lua client app is unnecessary.
- `libs/lgtm-ui`: shared Svelte diff, comment composer, thread cards, author badge, and review styling.
- `internal/review`: review validation, checkout access, and GitHub submission.
- `internal/storage`: SQLite/PostgreSQL transactions, relational entity storage, SQL migrations, and legacy import.
- `proto` and `gen/go`: the versioned protobuf contract and generated server/client code.
- `internal/transport`: generated `lgtm.v1.ReviewService` methods, status handling, and a Connect proxy.
- `internal/hosting`: local HTTP-to-gRPC gateway and static UI hosting.
- `scripts`: client integration checks and repeatable OSS review fixtures.

See [API.md](API.md) for request operations. Both clients call typed protobuf methods through the same gRPC server. The HTTP proxy uses the generated gRPC client.

## Alternatives and scope

The contract lives in `proto/lgtm/v1/review.proto`. Generated Go and TypeScript clients share its message types and enums. The browser uses binary protobuf through Connect. Neovim uses protobuf JSON through curl. Both HTTP paths call the generated Go gRPC client. SvelteKit's static adapter lets the CLI serve the UI without a Node server.

GitHub drafts live locally until submission. Submission uses GitHub's [create review endpoint](https://docs.github.com/en/rest/pulls/reviews) with `COMMENT`, `APPROVE`, or `REQUEST_CHANGES`. Editing existing GitHub pending reviews, deleting comments, synchronizing thread resolution, notifications, and provider support beyond GitHub are outside this experiment.

## Experiments and learnings

- Local state survives restarting the service; promotion preserves the comment's author. Both clients read and write the same database workspace.
- GitHub mock tests verify mixed human/agent payloads, remote review IDs, head checks, remote errors, all three review actions, summary validation, and duplicate submission guards. Actual remote writes remain untested: no target PR was provided for publishing test reviews.
- GitHub discussion and diffs are paginated. Binary and truncated GitHub patches cannot render complete diffs.
- Worktree diffs compare tracked files against HEAD, including staged and unstaged changes. Per-file diffs also include untracked text files. Local comments use current line numbers; no re-anchoring across edits is attempted.
- Existing checkout files are available through the source view. Comments in PR diff hunks become inline GitHub comments. Other promoted notes become review summary sections with file, line, side, author, and attribution preserved. Select the corresponding PR checkout before commenting on it.
- SQLite and PostgreSQL use relational tables, foreign keys, and transactions. Workspace locks serialize mutations across server processes. The filesystem and GitHub cannot form one transaction: a lost API response or a persistence failure after a successful remote write may require inspecting GitHub before retrying. Durable remote reconciliation remains a follow-up.
- Both listeners bind to loopback. HTTP mutations require JSON and reject cross-origin browser requests. Remote network hosting, TLS, authentication, and multi-user access are outside scope.
- Environment diagnosis: Difflab scaffold MCP worked; CLI 0.8.1 and GitHub authentication are available. The repo has no Difflab project link; it was not needed to scaffold/run this PoC. Project mise configuration resolved the missing Go version selection.

## Compare with main at branch creation

The immutable original base is `386919a863bed440d232e70e3aeb29937781c57c`, confirmed by the user from `origin/main`. Run `git diff 386919a863bed440d232e70e3aeb29937781c57c HEAD` for committed changes. During implementation, files are uncommitted; also inspect `git status --short` and the working files.

## Disposition and recovery

Keep this branch for evaluation. Next experiments: diff re-anchoring and recoverable remote submission. Once reviewed, use `poc freeze` to create an annotated tag. No commit, push, tag, or merge was made by this task.

Stop the service with Ctrl-C. Saved reviews remain in the selected database. Restarting against the same workspace restores them. Stop SQLite writers before copying the database and its WAL files. Use PostgreSQL backup tools for a PostgreSQL deployment. If a remote submission failed ambiguously, check the PR on GitHub before retrying.

Neovim regression checks exercise the actual CodeDiff buffers, inline multiline cards, replies from source anchors, and draft-preserving submission retries. GitHub mock tests cover empty comment summaries, own-PR approval guards, and detailed validation errors.

LGTM binds `c` when CodeDiff creates or replaces a diff buffer, including one-sided added or deleted files. It releases its mappings when you leave the LGTM tab and restores them when you return. Other review plugins retain their mappings in their own tabs.
