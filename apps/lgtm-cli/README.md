# lgtm-cli

LGTM includes a web UI and gRPC server in the `lgtm` executable.

```sh
cargo install lgtm-cli
lgtm up
lgtm status
lgtm down
```

`up` starts a managed background process and opens the UI at `http://127.0.0.1:8080`. Use `--no-open` to suppress the browser, `--port` to select a different port, or `--foreground` to run in the terminal. No Node, Bun, Buf, or separate backend installation is needed at runtime.

State and logs are stored in the platform's local state directory under `lgtm`. Set `LGTM_STATE_DIR` or pass `--state-dir` to choose another directory. A repeated `up` reuses the running instance. `down` shuts down both UI and RPC serving.

From the repository, run `mise run dev` for the backend watcher and Vite, or `mise run //apps/lgtm-cli:bundle` before a raw Cargo build. Only this CLI package is published; the private backend and web app are bundled as implementation code and static assets.
