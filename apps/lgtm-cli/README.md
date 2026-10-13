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

State and logs are stored in the platform's local state directory under `lgtm`. Set `LGTM_STATE_DIR` or pass `--state-dir` to choose another directory. A repeated `up` reuses the running instance. `down` shuts down both UI and RPC serving.

From the repository, run `mise run dev` for the backend watcher and Vite, or `mise run //apps/lgtm-cli:bundle` before a raw Cargo build. Only this CLI package is published; the private backend and web app are bundled as implementation code and static assets.
