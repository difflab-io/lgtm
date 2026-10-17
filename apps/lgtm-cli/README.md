# lgtm-cli

LGTM bundles a dummy web UI and greeting/health RPC router in the lgtm executable.

    cargo install lgtm-cli
    lgtm up --no-open
    lgtm status
    lgtm review --no-open
    lgtm down

The up command starts/reuses the daemon; --port selects its port and --foreground runs in the terminal. Review starts/reuses that daemon and opens the dummy UI for the encoded current path. The --no-open flag prints the URL. LGTM_HOME selects runtime files: server.pid, server.port and server.log. Status is filesystem read-only.

## Review command scaffold

Argument shapes exist for session start/show/complete/export/refresh/submit, sessions list and comments add/reply/list/resolve/reopen. These commands report that backend review RPCs are not implemented. They do not create review data, connect to SQLite/PostgreSQL, consult forge fixtures or create runtime/database files.

Context, attribution, anchor and historical-session flags are reserved shapes for future backend requests. The CLI does not normalize Git remotes or validate domain identities. Backend models/services will own this work behind future RPCs.

The CLI contains no review DB drivers or embedded ReviewService. Only the current RPC router, generated protocol code and static web UI are bundled. Review UI and live forge integration remain deferred.

## Development

Run mise run //apps/lgtm-cli:bundle before raw Cargo builds. Normal lifecycle tasks invoke it automatically. Tests cover daemon lifecycle, URL encoding, argument shapes and the absence of persistence side effects from scaffold commands. Client commands preserve stdout and do not initialize server logging.
