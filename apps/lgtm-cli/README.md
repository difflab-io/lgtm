# lgtm-cli

The `lgtm` executable includes the backend, persistent database and dummy web assets. Review commands use the backend RPC API; only the server opens the database or calls GitHub.

```sh
lgtm up --no-open
lgtm status --json
lgtm down
```

`up --port 0` chooses a free port. `up --foreground --no-open` runs in the terminal. `$LGTM_HOME` selects the runtime directory (default `~/.lgtm`), including PID/port/log files and `reviews.sqlite`. Set `LGTM_DATABASE_URL` to configure server storage. Client commands discover the daemon or use `LGTM_RPC_URL`, including `/rpc` for the bundled daemon and no prefix for the standalone backend.

## Notes and ignored directory review

Start the backend first. Set Git `user.name` and `user.email`, or `LGTM_NAME` and `LGTM_EMAIL`, for human attribution. Agents use `--as NAME --model MODEL --provider PROVIDER`; `--identity UUID` selects an existing actor. GitHub posting credentials are separate.

```sh
lgtm session start --notes .difflab/plans/my-plan --json
lgtm comments add --notes .difflab/plans/my-plan --file PLAN.md --line 12 --body 'Clarify the acceptance test' --json
lgtm comments list --notes .difflab/plans/my-plan --json
lgtm session export --notes .difflab/plans/my-plan --output review.md
lgtm session complete --notes .difflab/plans/my-plan --json
```

Use `comments add --notes FILE --line N --body TEXT` for ordinary saved-file notes; no visible review is needed. Explicit directory roots include ignored and untracked files. A root may itself be a symlink, as `.difflab` commonly is. Nested symlinks cannot escape the selected physical root. Paths in comments are relative to that root. Local notes and reviews need no GitHub credentials or browser. Each comment is saved immediately; completion ends the active session while retaining its conversation. Starting again creates a successor session.

Use `comments edit ID --body TEXT`, `comments delete ID`, `comments reply THREAD --body TEXT`, `comments resolve THREAD` and `comments reopen THREAD` with the same context and actor flags. `comments promote --notes PATH --target-session UUID --comment UUID --file TARGET_PATH` (or `--general`) explicitly copies selected notes to review drafts, preserving their source attribution. Export uses the shared backend Markdown formatter for notes, local reviews and GitHub conversations without changing their state.

A confirmed file or directory move can be recorded after moving it on disk:

```sh
lgtm session refresh --notes /new/absolute/path --moved-from /old/absolute/path --json
```

The old location must be absent and the destination must exist. A copy is rejected. Review and thread IDs, replies, attribution, and original anchors remain intact. For exact retry from a script, retain `--expected-version VERSION --request-key KEY` with the original payload. Notes at a file path do not create a visible review; an explicit directory review resumes by its original review ID after a root move.

Exports use numbered Markdown feedback, compact file/range locations, `~` for old-side lines, useful replies, original excerpts for lost locations, and optional overall feedback. They omit internal IDs and synchronization metadata. Local completion accepts `--body` for final feedback and has no GitHub outcome.

## Git and GitHub reviews

`--mode working|staged|branch --path CHECKOUT` selects a local comparison. `--mode branch --base BASE` compares the merge base with HEAD; `--base` requires branch mode. `--branch NAME` selects the checked-out local branch context; `--pr NUMBER --repo https://github.com/OWNER/REPO --path CHECKOUT` selects GitHub. A full GitHub PR URL also works. Start/select a session, then use the same comment commands. `session refresh` imports GitHub state; `session submit --outcome approve|reject|comment --body TEXT --request-key KEY` submits the review and optional final comment. GitHub permission and self-review restrictions still apply. Retain a request key when retrying a known request; uncertain writes require `session reconcile --session UUID` (or the same context flags) rather than blind resubmission.

The backend obtains credentials from the local `gh` account; run `gh auth login` as the same OS user as the backend. Tokens are not sent to the editor or stored in review exports.

## Machine output and browser compatibility

`--json` returns one JSON value on stdout and a nonzero process status on errors. Diagnostics stay on stderr. Session/comment commands never launch a browser. `lgtm review` retains the dummy browser entry point; `review --no-open` prints its URL. `review --json` starts a review through RPC without opening the browser.

## Neovim and development

See [lgtm.nvim setup and usage](../lgtm.nvim/README.md). The plugin calls Connect JSON directly; the CLI uses generated Rust gRPC clients against the same backend.

Run checks through Premise, for example `premise run lgtm-cli:test`. The bundle task embeds backend/database sources and generated protobuf code so the published CLI does not depend on unpublished workspace crates. App tests cover component/argument contracts; real-process workflows live in [e2e/cli](../../e2e/cli/), with their own Mise config, and run with `premise run e2e:cli`.

PR line/file comments and replies sync to a private pending GitHub review after
the backend saves them locally. Adding a comment never submits the review. JSON
results include `syncError` when local persistence succeeded but GitHub sync did
not. Use `lgtm comments sync COMMENT_ID --pr PR_URL --json` to sync an existing
local draft or recover an accepted draft after a lost response. General PR
comments stay local until explicit review submission.
