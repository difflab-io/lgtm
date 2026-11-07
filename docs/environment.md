# LGTM — environment variables

## Overview

LGTM components (`lgtm-cli`, `lgtm-forge`, `lgtm-desktop`, `lgtm-config`, `lgtm-lsp`, and the `e2e/` test harnesses) read environment variables to configure runtime paths, database connections, RPC endpoints, GitHub credentials, and isolated E2E test behavior.

## Runtime & configuration

| Variable | Component | Default | Description |
| --- | --- | --- | --- |
| `LGTM_HOME` | `lgtm-cli`, `lgtm-forge`, `lgtm-config`, `lgtm-env` | `~/.lgtm` | Overrides the LGTM state, snapshot, database, managed `mise` binary (`$LGTM_HOME/bin/mise`), and global configuration directory (`$LGTM_HOME/config.json`, `$LGTM_HOME/lgtm.db`, `$LGTM_HOME/snapshots`). |
| `LGTM_MISE_INSTALLER_URL` | `lgtm-env` | `https://mise.run` | Overrides the installer script URL used when `MiseService` bootstraps a missing `mise` binary into `$LGTM_HOME/bin/mise`. |
| `LGTM_DATABASE_URL` | `lgtm-forge`, `lgtm-cli` | `sqlite://$LGTM_HOME/lgtm.db?mode=rwc` | Explicit SeaORM database connection string (`sqlite://...` or `postgres://...`). Takes precedence over `DATABASE_URL`. |
| `DATABASE_URL` | `lgtm-forge`, `lgtm-cli` | `sqlite://$LGTM_HOME/lgtm.db?mode=rwc` | Fallback database connection URL when `LGTM_DATABASE_URL` is unset. |
| `LGTM_BACKEND_ADDR` | `lgtm-forge` | `127.0.0.1:8080` | Socket address (`<ip>:<port>`) bound by the standalone `lgtm-forge` server binary. |
| `LGTM_RPC_URL` | `lgtm-cli` | `http://127.0.0.1:<daemon-port>/rpc` | Explicit Connect RPC endpoint URL used by `lgtm-cli` instead of discovering or auto-starting the background daemon. |
| `RUST_LOG` | `lgtm-forge`, `lgtm-cli` | `info` | `tracing_subscriber::EnvFilter` directive controlling structured log verbosity. |

## GitHub & remote forge integration

| Variable | Component | Default | Description |
| --- | --- | --- | --- |
| `GITHUB_TOKEN` | `lgtm-forge` | `gh auth token` fallback | Personal access token or GitHub App token used by `octocrab` for REST and GraphQL pull request operations. |
| `GH_TOKEN` | `lgtm-forge` | `gh auth token` fallback | Fallback GitHub token read when `GITHUB_TOKEN` is unset. |
| `GITHUB_API_URL` | `lgtm-forge` | `https://api.github.com` | Custom GitHub REST/GraphQL base URI (for GitHub Enterprise or local proxy servers). |

## Mise & LSP subprocess environment

When `lgtm-env` (`MiseService`) and `lgtm-lsp` (`LgtmLspManager`) invoke or bootstrap `mise` for a workspace, they scope the child process environment to the workspace's generated `<workspace>/.lgtm/mise.toml`:

| Variable | Set by | Value | Purpose |
| --- | --- | --- | --- |
| `MISE_INSTALL_PATH` | `MiseService` | `<lgtm_home>/bin/mise` | Target binary path passed to the `mise` installer script when `mise` is not yet installed on the machine. |
| `MISE_CONFIG_FILE` | `MiseService`, `LgtmLspManager` | `<workspace>/.lgtm/mise.toml` | Directs `mise` to load the LGTM-synthesized workspace configuration without modifying any user-owned root `mise.toml`. |
| `MISE_GLOBAL_CONFIG_FILE` | `MiseService`, `LgtmLspManager` | `<workspace>/.lgtm/mise.toml` | Prevents personal global Mise tool definitions from conflicting with workspace-managed LSP toolchains. |
| `MISE_TRUSTED_CONFIG_PATHS` | `MiseService`, `LgtmLspManager` | `<workspace>/.lgtm/mise.toml` | Marks the workspace's `.lgtm/mise.toml` as trusted for non-interactive task and LSP execution. |
| `MISE_NOT_FOUND_AUTO_INSTALL` | `MiseService`, `LgtmLspManager` | `false` | Disables implicit background installs when `lsp.auto_install = false` or during initial `mise exec` probes. |
| `MISE_EXEC_AUTO_INSTALL` | `MiseService` | `false` | Ensures `mise exec` probes fail fast before `LgtmLspManager` decides whether to run explicit `mise install`. |
| `MISE_TASK_RUN_AUTO_INSTALL` | `MiseService` | `false` (when `lsp.auto_install = false`) | Prevents `mise run` from installing missing tools when automatic installation is disabled. |

## End-to-end test harness (`e2e/`)

| Variable | Component | Default | Description |
| --- | --- | --- | --- |
| `LGTM_IPC_BRIDGE` | `lgtm-desktop` (`e2e-bridge` feature) | Unset | Set to `1` by the Playwright desktop E2E fixture when spawning `lgtm-desktop --ipc-bridge <home>` so `workspace_finish_quit` records shutdown without terminating the stdio bridge before the response flushes. |
| `LGTM_E2E_PERMANENT_DELETE` | `lgtm-desktop` (`e2e-bridge` feature) | Unset | Set to `1` by the Playwright desktop E2E fixture so `WorkspaceService::delete` removes temporary test files directly instead of moving them into the host user's OS Trash (`~/.Trash`). Compiled only under `#[cfg(any(test, feature = "e2e-bridge"))]`. |
| `LGTM_E2E_LSP_SCRIPT` | `lgtm-config` | Unset | Optional Node.js mock language server script path injected by the Playwright desktop E2E fixture so built-in language presets execute the deterministic mock LSP server while keeping `~/.lgtm/config.json` free of test-only language overrides. |
| `LGTM_GITHUB_TEST_MODE` | `lgtm-forge` | Unset | When set to `1`, prevents `octocrab` from falling back to the developer's local `gh auth token` during offline cassette replay tests. |
| `LGTM_GITHUB_TEST_API_URL` | `lgtm-forge` | Unset | Overrides the GitHub API base URI to point at the local E2E cassette recording/replay proxy server. |
| `LGTM_E2E_GITHUB_MODE` | `e2e/common` | `replay` | Selects GitHub E2E transport mode: `replay` (strict offline cassette matching), `record` (live execution saving updated cassettes), or `live` (live verification without overwriting cassettes). |
| `LGTM_E2E_GITHUB_REPOSITORY` | `e2e/common` | `skapoor8/lgtm-e2e` | Permanent private GitHub repository used during `record` and `live` E2E runs. |
| `LGTM_E2E_GITHUB_TOKEN` | `e2e/common` | Stored `gh` token | Primary GitHub token used for `record` and `live` E2E runs. |
| `LGTM_E2E_GITHUB_SECONDARY_TOKEN` | `e2e/common` | Stored `gh` token | Secondary reviewer GitHub token used for multi-actor PR review E2E journeys. |
| `LGTM_E2E_GITHUB_PRIMARY_USER` | `e2e/common` | `skapoor8` | Primary GitHub login used to resolve `gh auth token --user` in live/record mode. |
| `LGTM_E2E_GITHUB_SECONDARY_USER` | `e2e/common` | Unset | Secondary GitHub login used to resolve `gh auth token --user` in live/record mode. |
| `LGTM_E2E_NVIM_PROFILES` | `e2e/nvim` | `required` | Comma-separated Neovim E2E profile list (`required`, `kak`, or `required,kak`). |

## References

- [Software architecture](architecture/index.md).
- [Desktop application architecture](architecture/desktop/index.md).
- [Configuration library architecture](architecture/config/index.md).
- [Environment & Mise library architecture](architecture/env/index.md).
- [LSP SDK architecture](architecture/lsp/index.md).
- [End-to-end testing architecture](architecture/e2e/index.md).
- [E2E suite guide](../e2e/README.md).
