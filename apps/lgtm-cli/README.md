# lgtm-cli

The `lgtm` executable bundles the hello-review backend, dummy DB package and Svelte page into one application. Review persistence and Git/forge operations are not implemented.

## Application lifecycle

```sh
lgtm up --no-open
lgtm status --json
lgtm ui
lgtm ui --no-open --json
lgtm down
```

`up --port 0` selects an available port; `up --foreground --no-open` serves in the terminal. `ui` starts or reuses the daemon, prints its URL and opens the browser unless `--no-open` is supplied. Browser-launch failure leaves the URL available for manual access.

`LGTM_HOME` selects the runtime directory, defaulting to `~/.lgtm`, for PID, port and log files. No database is opened and `LGTM_DATABASE_URL` is unused. `LGTM_RPC_URL` overrides client discovery: use the daemon URL with `/rpc`, or the standalone backend URL without that prefix.

## Review demonstration

```sh
lgtm up --no-open
lgtm review list --json
```

`review list` calls `ReviewService.HelloReview` through the generated Rust client and labels its result with `demo: true`; it does not return saved reviews. The browser page calls the same RPC and keeps display fixtures in frontend services.

## Future command shape

```text
lgtm commit [--as <agent-name>] [--local] [-m <message>]
lgtm push [--local]
lgtm review list [--local]
lgtm review new [--local]
lgtm review close <review-id> --with <comment|reject|approve|request> [--body <text>]
lgtm comment <body> [--review <review-id>] [--local] [--as <agent-name>]
```

Apart from the list demonstration, these commands return a clear unimplemented error and a nonzero exit status. They do not create commits, push branches, save reviews or publish comments, with or without `--local`. The intended local boundary leaves checkout files, index, HEAD, refs and the upstream forge untouched. This reset does not yet implement LGTM-managed Git storage or isolated agent workspaces.

`--json` is global: results and errors are JSON on stdout, with diagnostics on stderr. The old `session` and plural `comments` commands are removed.

## Development

Run checks through Premise, for example `premise run //apps/lgtm-cli:test`. The bundle task embeds backend/DB sources, generated protocol code and static web assets so the published CLI does not depend on unpublished workspace packages. See [development](../../docs/development.md), [CLI architecture](../../docs/architecture/cli/index.md) and [E2E](../../e2e/README.md).
