---
base_commit: '386919a'
---

# PoC: lgtm — GPUI + Tauri + TanStack diff viewer parity

> Experimental branch only. Do not merge into production or publish as a release.

## Purpose

Can three UI stacks — GPUI (native Rust), Tauri (web-view in Rust), and TanStack Start (React SPA on Bun) — present a near-identical minimal code-review UI backed by a single Rust Connect-JSON service that reads a local git worktree?

Success criterion: all three apps, launched against the same OSS fixture repo, show the same list of changed files on the left and the same raw diff text for the clicked file on the right.

Explicitly out of scope: syntax highlighting, split-pane layouts, side-by-side diffs, keyboard navigation, comment threads, user/agent attribution, PR integration. Those belong to follow-up PoCs.

## Current status

- **Backend (`lgtm-grpc`)**: implemented, verified end-to-end against the ripgrep fixture via curl.
- **Database (`lgtm-db-client`)**: opens `.tmp/db.sqlite` on server start; `reviews` table scaffolded.
- **TanStack SPA**: two-pane layout + Connect-JSON fetch client; `bun run build` clean, `bun run dev` serves.
- **Tauri shell**: Rust shell builds; `placeholder/` vanilla web frontend mirrors the SPA using the same CSS tokens.
- **GPUI app**: source written (two-pane `Render` impl + stub data). **Build is blocked** by a missing macOS `metal` compiler — the host has Command Line Tools but not full Xcode; `gpui_apple`'s build script requires `xcrun metal`. Install full Xcode and the build will proceed. The GPUI crate is isolated into its own workspace (see "Known issues") so this does not affect the other projects.

Nothing has been committed. The PoC is runnable locally; see tasks below.

## Setup and tasks

Required tools:

- `rust` 1.88+ (`cargo` 1.98 verified)
- `bun` 1.4.2
- `pm` / `premise` CLI
- **For GPUI only**: full Xcode (installs `xcrun metal`). CLT alone is not enough.

Workspace layout:
- Rust Cargo workspace at the repo root (`Cargo.toml`) with 3 members: `apps/lgtm-desktop-tauri`, `libs/lgtm-grpc`, `libs/lgtm-db-client`.
- `apps/lgtm-desktop-gpui` is a **standalone** workspace (its own `Cargo.lock`) because Zed's gpui pulls `cbindgen` which needs `toml ^0.8.8`, while Tauri 2.11.6 locks the shared `toml` dep to `0.8.2` via `webkit2gtk → gdk-sys → system-deps`.
- `apps/lgtm-web-tanstack-spa` is a standalone bun package.
- `.tmp/` is gitignored: `db.sqlite` + `repos/<fixture>` (`.tmp/repos/ripgrep` is cloned here).

| Task                  | Command                                        | Observed result (2026-10-09)                                                                   |
| --------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Build (fan-out)       | `pm run //...:build`                           | ok (3 Rust crates + web app)                                                                   |
| Test (fan-out)        | `pm run //...:test`                            | ok (template unit tests)                                                                       |
| Build backend         | `cargo build -p lgtm-grpc`                     | ok                                                                                             |
| Build web             | `mise run //apps/lgtm-web-tanstack-spa:build`  | ok (vite + tsc clean)                                                                          |
| Build Tauri           | `cargo build -p lgtm-desktop-tauri`            | ok                                                                                             |
| Build GPUI            | `mise run lgtm:build:gpui`                     | **fails**: `xcrun: error: unable to find utility "metal"`. Needs full Xcode on macOS.          |
| Serve backend         | `mise run lgtm:serve`                          | ok — listens on `127.0.0.1:50051`; curl probes return correct JSON (see smoke-test below)      |
| Run web               | `mise run lgtm:run:web`                        | ok — vite dev server up on :3000                                                               |
| Run Tauri             | `mise run lgtm:run:tauri`                      | compiles; GUI not launched in this headless pass                                               |
| Run GPUI              | `mise run lgtm:run:gpui`                       | gated on the blocked GPUI build                                                                |

### Backend smoke-test

With `mise run lgtm:serve` running:

```
curl -s -X POST http://127.0.0.1:50051/lgtm.v1.LgtmService/GetChangedFiles \
  -H 'Content-Type: application/json' \
  -d '{"repo_path":"<abs>/.tmp/repos/ripgrep","base_ref":"HEAD~5"}'
# → {"files":[{"path":"Cargo.lock","status":"modified"},…6 more…]}

curl -s -X POST http://127.0.0.1:50051/lgtm.v1.LgtmService/GetFileDiff \
  -H 'Content-Type: application/json' \
  -d '{"repo_path":"<abs>/.tmp/repos/ripgrep","base_ref":"HEAD~5","path":"crates/ignore/src/gitignore.rs"}'
# → {"diff_text":"diff --git a/crates/ignore/src/gitignore.rs …"}
```

Error path (empty repo_path):

```
{"code":"invalid_argument","message":"invalid argument: repo_path must not be empty"}
```

## Code map

```
Cargo.toml                 Rust workspace (members: apps/lgtm-desktop-tauri, libs/*)
premise.yaml               Workspace manifest (projects list)
mise.toml                  Workspace-level mise config + lgtm:{serve,run:*,build:*} tasks
apps/
  lgtm-desktop-gpui/         Standalone Rust workspace. Zed gpui + gpui_platform.
    Cargo.toml                 Pins gpui + gpui_platform to zed-industries/zed@f0b38ac.
    src/main.rs                Platform boot: open window, mount ReviewView.
    src/review.rs              Two-pane Render impl.
    src/stub.rs                Hard-coded sample data (swap for lgtm-grpc client once Xcode is installed).
  lgtm-desktop-tauri/        Tauri 2 Rust shell + placeholder/ vanilla web frontend.
    placeholder/index.html     Two-pane markup.
    placeholder/review.css     Shared palette with the SPA (lgtm-* class names).
    placeholder/review.js      Vanilla client, parity mirror of lgtm-client.ts + ReviewLayout.tsx.
  lgtm-web-tanstack-spa/     TanStack Start SPA (bun + vite).
    src/lib/lgtm-client.ts     Connect-JSON client (fetch wrapper).
    src/lib/review-state.ts    React hook: files/selection/diff/loading/error.
    src/components/ReviewLayout.tsx  Two-pane component.
    src/styles/review.css      Source-of-truth palette.
    src/routes/index.tsx       Route composing the layout; config via URL params.
libs/
  lgtm-grpc/                 Backend crate — Connect-JSON unary service.
    proto/lgtm.proto           Schema source of truth. Hand-synced to serde types.
    src/proto.rs               Serde message types.
    src/git.rs                 git2-backed changed-files + unified-diff primitives.
    src/service.rs             LgtmService trait + DefaultService.
    src/server.rs              axum Router (CORS, two POST routes).
    src/error.rs               ServiceError + Connect error envelope.
    src/bin/lgtm-grpc.rs       Binary: open sqlite, bind 127.0.0.1:50051.
  lgtm-db-client/            Rusqlite wrapper opening .tmp/db.sqlite.
    src/db.rs                  Db::open_at, Db::record_review.
    src/schema.rs              Idempotent init SQL (reviews table).
.tmp/                      gitignored
  repos/ripgrep/             BurntSushi/ripgrep fixture (--depth 50).
  db.sqlite                  Created on first server start.
```

Backend contract (see `libs/lgtm-grpc/proto/lgtm.proto`):

- `GetChangedFiles(repo_path, base_ref) -> list<ChangedFile{path, status}>`
- `GetFileDiff(repo_path, path, base_ref) -> {diff_text}`

The Rust `LgtmService` trait mirrors the proto shape so in-process callers (GPUI, tests) and HTTP handlers share one surface. The TS and vanilla JS clients hand-write the same message shapes.

## Alternatives

The three apps are the alternatives — they share one backend and target the same UI, differing only in UI stack. Comparison is qualitative: fidelity of layout, startup time, perceived latency, developer experience.

Three distinct frontend tactics are used, deliberately:

- **TanStack SPA**: full React + Vite build. Modules split into `lib/`, `components/`, `routes/`, `styles/`.
- **Tauri shell**: single HTML file + vanilla ES-module JS; no bundler. Markup and CSS mirror the SPA for parity without a shared build pipeline.
- **GPUI**: native widget tree. Palette constants pulled from the shared CSS vars; layout structure (two columns, header + body) mirrors the other two. Blocked on the Metal toolchain prerequisite.

## Experiments

| Approach             | What changed                                                                       | How to run                              | Result                                   |
| -------------------- | ---------------------------------------------------------------------------------- | --------------------------------------- | ---------------------------------------- |
| Premise scaffolding  | 5 projects generated via local template registries + Cargo workspace                | `pm run //...:build`, `pm run //...:test` | ok                                     |
| Connect-JSON backend | axum + serde + git2; two unary endpoints under `/lgtm.v1.LgtmService/*`             | `mise run lgtm:serve` + curl           | ok — correct JSON for valid + error paths |
| TanStack Start       | React SPA + fetch client, hook-driven state, URL-param config                      | `mise run lgtm:run:web`                 | ok — vite served, build clean            |
| Tauri 2 placeholder  | HTML + vanilla JS mirror of the SPA, same CSS vars                                 | `mise run lgtm:run:tauri`               | compiles; not GUI-launched in this run    |
| GPUI two-pane view   | Zed gpui + gpui_platform pinned to `zed-industries/zed@f0b38ac`; stub data         | `mise run lgtm:run:gpui`                | **blocked** — needs full Xcode (`xcrun metal`) |

## Known issues

- **GPUI needs full Xcode on macOS**. `gpui_apple`'s build script shells out to `xcrun metal` to compile shaders. Command Line Tools don't ship `metal`. Install Xcode from the App Store and `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`. The source code is complete and matches Zed's current `hello_world` example API.
- **GPUI must live in its own Cargo workspace**. The shared root workspace locks `toml` at 0.8.2 (via `tauri 2.11.6 → webkit2gtk → gdk-sys → system-deps 6.2.2 → toml ^0.8`). Zed's `gpui_apple` pulls `cbindgen 0.28` which requires `toml ^0.8.8`. The clean fix is excluding the GPUI app from the root workspace (`Cargo.toml` has `exclude = ["apps/lgtm-desktop-gpui"]` with a `[workspace]` section in the app's own manifest). If Tauri bumps to a release that picks up a newer `webkit2gtk`/`gdk-sys`, this exclusion can be removed.
- `~/Projects/premise-cargo/premise.yaml` has `field ci not found in type core.Template` — `pm template ls` errors. `pm generate` is also TTY-only (bubbletea). For this PoC I copied the templates directly and ran the registry's name substitutions by hand.

## Learnings

- Rust's "Connect RPC" story is still immature; published `connectrpc` crates are experimental. For two unary endpoints, hand-written JSON unary over axum is correct — tiny, readable, and the wire protocol stays Connect-compatible (plain `POST`/JSON). Both Rust (future GPUI client via `reqwest`) and TS/vanilla-JS clients hand-write matching serde / TS types against `proto/lgtm.proto`.
- UI parity across three stacks is maintained by naming + palette, not a shared bundle. The SPA, Tauri HTML, and GPUI widgets all use `lgtm-sidebar` / `lgtm-fileitem` / `lgtm-diff-body` as semantic names and the same colour tokens, so visual drift is caught by eye immediately.
- SQLite via `rusqlite` with the `bundled` feature avoids system-library coupling and compiles cleanly from source on macOS.
- `git2` + `diff_tree_to_workdir_with_index` is the right primitive for "what changed since this ref" — it folds in untracked files and bypasses the index without needing `git` subprocess.

## Compare with main at branch creation

The `base_commit` is the full SHA of `origin/main` when this branch was created; do not update it when main moves. Compare this PoC with its starting point using `git diff <base_commit> HEAD`.

## Disposition

The remaining item is replacing GPUI's `stub.rs` with a `reqwest`-backed `lgtm-grpc` client (shape identical to the TS/vanilla clients). That is a 30-minute change once Xcode is installed and `gpui_apple` compiles.

Freeze with an annotated tag once GPUI is verified. Do not merge this branch. Follow-up PoCs should layer syntax highlighting, keyboard navigation, inline comments, and PR integration on top of whichever stack this comparison validates.
