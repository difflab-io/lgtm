# Review commands — software architecture

## Overview

The CLI manages review contexts, sessions and comments directly through the database service. `review` prepares a session and opens the existing browser page with encoded query parameters. The UI still consumes only `path`; it does not display sessions or comments.

## Requirement

### Functional

- Infer the repository/branch from cwd, or notes outside Git.
- Allow explicit PR, branch, notes, identity and path selection.
- Persist conversations across CLI processes and session completion.
- Preserve `up`, `down`, `status`, `serve`, browser suppression and server reuse.

### Non-Functional

- Never change Git files, index, refs or commits.
- Keep JSON/Markdown/URL output on stdout and context/stub diagnostics on stderr.
- Keep status discovery read-only, without creating a runtime directory/database.

## Design

### Components and Responsibilities

- `main` defines top-level dispatch; `reviews` defines Clap session/comment handlers.
- `context` provides injectable read-only Git discovery, canonical paths and identity selection.
- The bundled backend review module provides `AuthService` for requester identity and `ReviewService` for sessions/comments/threads; it calls the database library’s entity repositories.
- Filesystem `Store` selects runtime paths; it does not implement database operations.
- `ui` ensures server readiness and constructs the URL through `reqwest::Url`.

### Data and Control Flow

A command selects a context from cwd or explicit flags. Mutation commands resolve an actor from `--identity`, `--as`/`--model`, or Git name/email. Missing identity fails without invented attribution. Database commands initialize/migrate the selected database. Starting or adding a root may create the active session; list/show/export/reply/resolve do not create a missing context/session.

Completion publishes the origin session's drafts and records saved (notes), ready-to-address (branch), or an explicit PR outcome. The next start creates a different session while reusing the context's existing comments. A retry key returns the original completion.

PR start/refresh requires `LGTM_FORGE_FIXTURE`, parsed into the fake provider contract. The command emits STUB and never claims live freshness or posting. `submit` is PR-only and completes locally with unsubmitted status. A real forge adapter remains deferred.

For `review`, the handler prepares context/session, closes its database pool, then starts/reuses the local server and prints/opens a URL. `--no-open` suppresses the browser. The page may receive `path`, `repo` and `session`; only `path` is currently consumed.

### Interfaces and Contracts

```sh
lgtm comments add --notes ./docs --as 'Alice <alice@example.com>' --body 'Clarify this'
lgtm session complete --notes ./docs --as 'Alice <alice@example.com>'
lgtm session start --notes ./docs --as 'Alice <alice@example.com>'
lgtm session export --notes ./docs
lgtm sessions list --all --completed
lgtm review --notes ./docs --as 'Alice <alice@example.com>' --no-open
```

Context flags `--pr`, `--branch` and `--notes` are mutually exclusive. `--notes` without an argument uses cwd. Repository names normalize to lower-case HOST/OWNER/REPO; `--host` defaults to github.com. Repositories without origin use canonical Git common-directory identity. Detached HEAD requires an explicit context. Branch rename/reuse follows the selected full ref; it never automatically promotes local comments into a PR.

Normal commands need no session IDs. `--session` is a read-only historical selector for show/export. Thread IDs select a particular conversation. Replies cannot be independently resolved; a comment ID is not accepted as a thread ID.

`--all` disables inferred list filters. Explicit path/repository filters replace their inferred counterpart and combine when both are present. Notes include descendants on directory boundaries; workspace filters match the containing checkout. `--completed` includes historical sessions.

`LGTM_DATABASE_URL` selects the database. Otherwise commands use `LGTM_HOME/reviews.sqlite` or `~/.lgtm/reviews.sqlite`. Default builds support SQLite; optional PostgreSQL requires the postgres Cargo feature. IDs and author data are visible in JSON and Markdown exports.

## Implementation

### Command and storage boundaries

`reviews` handles CLI inputs; the backend owns application workflows, and lgtm-db owns entity repositories. CLI process tests use temporary stores and real databases. Fake Git/browser/forge boundaries and `--no-open` prevent unintended external actions. The existing daemon tests still cover readiness, reuse, encoding, shutdown and port failures.

### Export and compatibility

Session exports explicitly identify current conversation content rather than claiming a snapshot. Original bodies are preserved as Markdown. An explicit output path writes a file; otherwise output is stdout. Existing `up`/`down`/`status` behavior is independent of database initialization. The UI query contract extends the launcher without adding UI features.

## References

- [Command dispatch](../../../apps/lgtm-cli/src/main.rs), [handlers](../../../apps/lgtm-cli/src/reviews.rs), [context discovery](../../../apps/lgtm-cli/src/context.rs).
- [Database architecture](../db/index.md), [runtime Store](store.md), [CLI usage](../../../apps/lgtm-cli/README.md).
- [Process tests](../../../apps/lgtm-cli/tests/review_workflows.rs), [lifecycle tests](../../../apps/lgtm-cli/tests/lifecycle.rs).
