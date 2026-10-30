# CLI runtime store

The store owns PID, port and log files under `LGTM_HOME`, defaulting to `~/.lgtm`. Selecting a store does not create it; the server creates runtime directories when needed and restricts Unix access to the owner.

`server.pid` records process ownership, `server.port` records the listener and `server.log` captures background output. Discovery also probes server health so stale files do not identify an unrelated process as LGTM.

The runtime store contains no active review database in this skeleton. Existing `reviews.sqlite` files are left untouched and unused. Tests can select a temporary `LGTM_HOME` without changing the user's active instance.

See [daemon](daemon.md) and [store source](../../../apps/lgtm-cli/src/store.rs).
