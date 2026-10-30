# CLI server

The server hosts embedded Svelte assets and the backend router on one loopback listener. `/rpc` serves browser RPC calls, root service paths support native clients, and `GET /health` reports the process ID used for daemon discovery.

Startup creates runtime state and acquires PID ownership, binds the listener, builds the dummy DB/review service router and records the actual port. It does not open a database or initialize forge clients. Port `0` selects an available port.

GET/HEAD requests serve embedded assets, with unknown assets returning 404. Shutdown marks backend health stopped, drains connections within the existing bound and clears the recorded port. The same backend implementation is used by the standalone server and bundled CLI.

See [daemon](daemon.md), [runtime store](store.md) and [server source](../../../apps/lgtm-cli/src/server.rs).
