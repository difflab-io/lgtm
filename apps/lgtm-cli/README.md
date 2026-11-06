# lgtm-cli

The `lgtm` executable is the command-line review client, workspace task runner, background `lgtm-forge` RPC daemon manager, and `lgtm-desktop` workspace launcher. It links `lgtm-forge`, `lgtm-db`, `lgtm-config`, and `lgtm-env` directly as Cargo workspace dependencies.

```sh
lgtm open [<path>]
lgtm [<path>]
lgtm up [--port <port>] [--foreground] [--json]
lgtm status [--json]
lgtm down
lgtm run --list
lgtm run <task> [args...]
```

`lgtm open [<path>]` (or `lgtm [<path>]` with no subcommand) resolves `<path>` (defaulting to the current working directory) and launches the `lgtm-desktop` application rooted at that workspace directory.

`up --port 0` selects an available port for the background `lgtm-forge` RPC daemon; `up --foreground` serves in the terminal. `status` reports `running`, `pid`, `server_url`, and `rpc_url`. `down` stops the running daemon.

`LGTM_HOME` selects the runtime directory, defaulting to `~/.lgtm`, for PID, port, log, snapshot, and SQLite database files. `LGTM_RPC_URL` overrides client discovery: use the daemon URL with `/rpc`, or the standalone `lgtm-forge` URL without that prefix. `LGTM_DESKTOP_BIN` overrides the `lgtm-desktop` executable path used by `lgtm open`.

## Git and GitHub reviews

```text
lgtm add <paths>... [--local]
lgtm commit [--as <agent-name>] [--local] [-m <message>]
lgtm push [--local]
lgtm review list [--local] [--all]
lgtm review new [--local] [--as <agent-name>]
lgtm review refresh [<review-id-or-name>] [--local]
lgtm review show [<review-id-or-name>] [--local] [--group-by <file|session>] [--unresolved]
lgtm review close [<review-id-or-name>] --with <comment|reject|approve|request> [--body <text>] [--as <agent-name>]
lgtm comment new <body> [--review <review-id-or-name>] [--file <path>] [--start-line <line>] [--line <line>] [--reply-to <comment-id>] [--local] [--as <agent-name>]
lgtm comment update <comment-id> [--body <text>] [--file <path>] [--start-line <line>] [--line <line>] [--resolved <true|false>] [--local] [--as <agent-name>]
lgtm comment delete <comment-id> [--local] [--as <agent-name>]
```

## Machine output

`--json` is global: results and errors are JSON on stdout, with diagnostics on stderr.

## Development

Run checks through Premise, for example `premise run //apps/lgtm-cli:test`. See [development](../../docs/development.md), [CI & release architecture](../../docs/ci.md), [CLI architecture](../../docs/architecture/cli/index.md), and [E2E](../../e2e/README.md).
