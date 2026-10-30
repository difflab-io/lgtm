# CLI daemon

The daemon starts, discovers and stops the process serving the bundled UI and API. `up` and `ui` start or reuse a healthy instance; `status` only reports discovery; `down` stops the identified instance.

Discovery checks the stored PID and port against `/health` before trusting a process. PID-file ownership prevents duplicate servers. The detached server writes to the runtime log, and startup errors identify that log for diagnosis. A stale PID alone does not authorize shutdown.

`ui` prints the application URL and opens the browser unless `--no-open` is supplied; JSON mode returns the URL as structured output. Browser-launch failure preserves the printed URL for manual access.

See [runtime store](store.md), [server](server.md) and [daemon source](../../../apps/lgtm-cli/src/daemon.rs).
