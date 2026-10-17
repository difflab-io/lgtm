# lgtm-cli

LGTM includes a web UI and gRPC server in the `lgtm` executable.

```sh
cargo install lgtm-cli
lgtm up
lgtm status
lgtm review
lgtm down
```

`up` uses the embedded `daemonize` library on macOS and Linux to start the UI and backend in one background process. It opens the UI at `http://127.0.0.1:8080`. Use `--no-open` to suppress the browser, `--port` to select a different port, or `--foreground` to run in the terminal. No Node, Bun, Buf, or separate daemon tool is needed at runtime.

`status` shows the UI, server, and browser RPC URLs. `status --json` exposes the same addresses for editor integration. `review` starts or reuses the server and opens `/?path=<encoded current directory>`. Use `review --no-open` to print that URL without opening a browser.

All runtime files live in `~/.lgtm`: `server.pid`, `server.port`, and `server.log`. A repeated `up` reuses the running server. `down` sends SIGTERM to stop both UI and API serving. `LGTM_HOME` selects an isolated store for testing.

From the repository, run `mise run dev` for the backend watcher and Vite, or `mise run //apps/lgtm-cli:bundle` before a raw Cargo build. Only this CLI package is published; the private backend and web app are bundled as implementation code and static assets.

## Review sessions and comments

Notes use a canonical path; local reviews use repository and branch. Each context
has one active session and persistent threads/comments. Completed sessions record
review metadata. New sessions reuse the same conversation without snapshots.

```sh
lgtm comments add --notes ./docs --as 'Alice <alice@example.com>' --body 'Clarify this'
lgtm comments list --notes ./docs
lgtm session complete --notes ./docs --as 'Alice <alice@example.com>'
lgtm session start --notes ./docs --as 'Alice <alice@example.com>'
lgtm session export --notes ./docs --output review.md
lgtm sessions list --all --completed
```

In Git, omitting context flags selects the current branch and Git identity:

```sh
lgtm comments add --body 'Check this constraint' --file src/lib.rs --line 1 --side new
lgtm comments reply THREAD --as 'ReviewBot <bot@example.com>' --model model-name --body 'Checking the race'
lgtm comments resolve THREAD
lgtm comments reopen THREAD
lgtm session complete --body 'Ready to address'
lgtm session start
lgtm review --no-open
```

Replace THREAD with the thread_id printed by add. Resolve/reopen applies to threads,
not individual replies. Anchor paths are relative; line anchors require a reviewed
commit and old/new side. --end-line adds an inclusive range. Original authors and
anchors stay recorded; later sessions mark anchors outdated rather than remap them.

Explicit selectors are --notes [PATH], --branch BRANCH and --pr OWNER/REPO#NUMBER.
They are mutually exclusive. --repo overrides repository inference; --host qualifies
forge shorthand. Detached HEAD needs an explicit context. --as requires Name <email>;
--identity selects an existing UUID, and --model records an agent author. Ambiguous
identity requires an explicit selection. No authentication is implied by attribution.

Comments are drafts until their originating session completes. Completion publishes
locally; it does not merge/close a PR. Notes record saved, branches ready-to-address,
and PRs require --outcome approve|reject|comment. An explicit --request-key makes a
completion retry return the original result. A different actor/body with the same
key fails. Completed metadata is fixed; comments and resolution stay current.

Show/export accepts --session ID for an earlier review. Exports include its originating
comments with current bodies/resolution and explicitly do not promise historical
conversation snapshots. Default selection is active, or latest completed. Explicit
--output writes a file; stdout otherwise contains only the Markdown export.

List infers both cwd and repository. --all disables inference. Explicit --path and
--repo filters override their inferred counterpart; --completed includes history.

## Database and forge limits

The default is LGTM_HOME/reviews.sqlite, otherwise ~/.lgtm/reviews.sqlite.
LGTM_DATABASE_URL overrides it, for example sqlite:///absolute/reviews.sqlite?mode=rwc
or a PostgreSQL URL with a postgres-enabled build. Default builds include SQLite;
build with --features postgres to also support PostgreSQL. Migration/foreign-key
checks run when database commands open the store. status remains filesystem read-only.

Live forge fetching/posting is not implemented. To test PR behavior, explicitly set
LGTM_FORGE_FIXTURE to a JSON file:

```json
{ "complete": true, "head": "reviewed-head", "base": "reviewed-base", "threads": [] }
```

```sh
LGTM_FORGE_FIXTURE=/tmp/forge.json lgtm session start --pr acme/repo#42 --as 'Alice <alice@example.com>'
LGTM_FORGE_FIXTURE=/tmp/forge.json lgtm session refresh --pr acme/repo#42
lgtm session submit --pr acme/repo#42 --as 'Alice <alice@example.com>' --outcome comment
```

Fixture use prints STUB. Submit publishes locally with unsubmitted status and no
invented forge IDs. The future provider must fetch every page, match direct scoped
remote IDs and retain known agent authors. Agent posting appends > Generated via
model-name to the remote payload only. Uncertain writes require reconciliation,
not blind retry. There is no background sync queue.

review prepares a context/session and opens a URL containing encoded path, repo
and session parameters. Only path is currently consumed by the greeting UI. The UI
does not load or edit these database records. --no-open prints the URL; --port
selects a port only when starting a new server. Other diagnostics go to stderr.
