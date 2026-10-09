---
base_commit: '386919a'
---

# PoC: lgtm — GPUI + Tauri + TanStack diff viewer parity

> Experimental branch only. Do not merge into production or publish as a release.

## Purpose

Can three UI stacks — GPUI (native Rust), Tauri (web-view in Rust), and TanStack Start (React SPA on Bun) — present a near-identical minimal code-review UI backed by a single Rust Connect-JSON service that reads a local git worktree? And can agents review the same artefact via a CLI against that service?

Success criterion:
- Three apps, launched against the same OSS fixture repo, show the same list of changed files on the left and the same colored unified diff for the clicked file on the right.
- A `lgtm` CLI drives the same backend to list files, print diffs, and create/reply/resolve comment threads under a user or agent identity.

Explicit non-goals (still): split-pane layouts, side-by-side diffs, keyboard navigation, in-app comment UIs, PR integration. Those belong to follow-up PoCs.

## Current status

- **Backend (`lgtm-grpc`)**: `GetChangedFiles` + `GetFileDiff` (git2-backed) + `UpsertUser` + `ListThreads` + `CreateThread` + `AppendComment` + `ResolveThread`.
- **Database (`lgtm-db-client`)**: `.tmp/db.sqlite` with `users`, `threads`, `comments` tables; one lifecycle unit test covers create → reply → resolve.
- **CLI (`apps/lgtm-cli`)**: `lgtm {files,diff,user upsert,threads,comment {add,reply,resolve}}`. Smoke-tested end-to-end against the ripgrep fixture.
- **TanStack SPA**: two-pane layout + Connect-JSON fetch client + diff coloring + Shiki syntax highlighting (github-dark theme, extension-driven language detection).
- **Tauri shell**: Rust shell builds; `placeholder/` vanilla web frontend mirrors the SPA layout with the same CSS tokens and the same diff coloring (no Shiki — intentional, keeps the dep surface at zero).
- **GPUI app**: two-pane view compiles and runs against stub data; diff lines render with inline color tints matching the SPA.

Comment UIs inside the apps are deliberately *not* in this cut; the CLI covers the "agents post review comments" use case, and the web/Tauri/GPUI apps are read-only for now.

## Setup and tasks

Required tools:

- `rust` 1.88+ (`cargo` 1.98 verified)
- `bun` 1.4.2
- `pm` / `premise` CLI
- **For GPUI only**: full Xcode. `xcode-select -s /Applications/Xcode.app/Contents/Developer` + `xcodebuild -license accept`.

Workspace layout:
- Rust Cargo workspace at the repo root (`Cargo.toml`): `apps/lgtm-cli`, `apps/lgtm-desktop-tauri`, `libs/lgtm-grpc`, `libs/lgtm-db-client`.
- `apps/lgtm-desktop-gpui` is a **standalone** workspace (its own `Cargo.lock`) — the Zed gpui dep graph locks `toml ^0.8.8` while Tauri 2.11.6 pins `webkit2gtk → gdk-sys → system-deps → toml 0.8.2`.
- `apps/lgtm-web-tanstack-spa` is a standalone bun package.
- `.tmp/` is gitignored: holds `db.sqlite` + `repos/<fixture>`.

| Task | Command | Observed result (2026-10-09) |
| --- | --- | --- |
| Build (fan-out, excl. GPUI) | `pm run //...:build` | ok |
| Test (fan-out, excl. GPUI) | `pm run //...:test` | ok |
| Backend build | `cargo build -p lgtm-grpc` | ok |
| CLI build | `cargo build -p lgtm-cli` | ok |
| Web build | `mise run //apps/lgtm-web-tanstack-spa:build` | ok (vite + tsc, Shiki chunked) |
| Tauri build | `cargo build -p lgtm-desktop-tauri` | ok |
| GPUI build | `mise run lgtm:build:gpui` | ok (needs full Xcode) |
| Serve backend | `mise run lgtm:serve` | ok — listens on `127.0.0.1:50051`; headless curl probes pass |
| Dev web + backend | `mise run lgtm:dev:web` | ok — Ctrl-C stops both |
| Run Tauri | `mise run lgtm:run:tauri` | compiles, GUI not launched in this headless pass |
| Run GPUI | `mise run lgtm:run:gpui` | ok (opens window; diff colored) |
| CLI | `mise run lgtm:cli -- <args>` or `cargo run -p lgtm-cli -- <args>` | ok |

### End-to-end CLI smoke (reproducible)

```
mise run lgtm:serve  &
./target/debug/lgtm files .tmp/repos/ripgrep --base HEAD~5
./target/debug/lgtm diff  .tmp/repos/ripgrep --base HEAD~5 --path crates/ignore/src/gitignore.rs

tid=$(./target/debug/lgtm comment add .tmp/repos/ripgrep \
  --path crates/ignore/src/gitignore.rs --line 361 --base HEAD~5 \
  --body "nit: pool capacity change could regress at 1 thread" \
  --as claude-code --as-kind agent)
./target/debug/lgtm comment reply --thread $tid \
  --body "benchmarked; no regression; LGTM" --as sid --as-kind user
./target/debug/lgtm threads .tmp/repos/ripgrep --base HEAD~5
./target/debug/lgtm comment resolve --thread $tid
./target/debug/lgtm threads .tmp/repos/ripgrep --base HEAD~5 --unresolved   # empty
./target/debug/lgtm threads .tmp/repos/ripgrep --base HEAD~5 --json         # full JSON
```

### Headless browser verification (2026-10-09)

Driving the SPA via `/Applications/Google Chrome.app/.../Google Chrome --headless --dump-dom`:
- 6 file-list items render with `lgtm-fileitem` classes.
- Rendered Cargo.lock diff contained **45 added / 34 removed / 120 context / 13 hunk / 5 header** lines, all with the right `lgtm-line-*` classes.
- Shiki produced **296 inline-color spans** on Cargo.lock (`.lock` is mapped to TOML).

## Code map

```
Cargo.toml                 Rust workspace (apps/lgtm-cli, apps/lgtm-desktop-tauri, libs/*)
premise.yaml               Workspace manifest
mise.toml                  Workspace mise config + lgtm:{serve,dev:web,run:*,build:*,cli}
apps/
  lgtm-cli/                  Agent-friendly CLI (binary name: `lgtm`).
    src/main.rs                clap command tree + env defaults.
    src/client.rs              Blocking reqwest wrapper around Connect-JSON.
  lgtm-desktop-gpui/         Standalone workspace; Zed gpui + gpui_platform.
    src/main.rs                Platform boot: open window, mount ReviewView.
    src/review.rs              Two-pane Render impl, colored diff lines.
    src/stub.rs                Hard-coded sample data (not yet swapped for lgtm-grpc).
  lgtm-desktop-tauri/        Tauri 2 shell + placeholder/ vanilla frontend.
    placeholder/index.html     Two-pane markup.
    placeholder/review.css     Shared palette with the SPA.
    placeholder/review.js      Vanilla client + diff coloring (mirror of SPA).
  lgtm-web-tanstack-spa/     TanStack Start SPA (bun + vite).
    src/lib/lgtm-client.ts     Connect-JSON client.
    src/lib/review-state.ts    Hook: files/selection/diff/loading/error.
    src/lib/diff-lines.ts      Unified-diff classifier (shared shape with GPUI/Tauri).
    src/lib/highlight.ts       Lazy Shiki singleton + extension→lang mapping.
    src/components/ReviewLayout.tsx  Two-pane component + <DiffBody> with Shiki.
    src/styles/review.css      Palette source of truth.
    src/routes/index.tsx       Route; URL params ?repo=, ?base=, ?backend=.
libs/
  lgtm-grpc/                 Backend crate — Connect-JSON unary service.
    proto/lgtm.proto           Source of truth. Hand-mirrored in src/proto.rs.
    src/proto.rs               Serde messages. Enums serialise as SCREAMING_SNAKE.
    src/git.rs                 git2 primitives.
    src/service.rs             LgtmService trait + DefaultService (holds Arc<Db>).
    src/server.rs              axum Router: 7 POST routes under /lgtm.v1.LgtmService.
    src/error.rs               ServiceError + Connect error envelope.
    src/bin/lgtm-grpc.rs       Binary: open sqlite, bind 127.0.0.1:50051.
  lgtm-db-client/            SQLite wrapper.
    src/db.rs                  Db::{upsert_user, create_thread, append_comment, resolve_thread, list_threads}.
    src/schema.rs              Idempotent init SQL (users, threads, comments; legacy reviews table kept).
.tmp/                      gitignored
  repos/ripgrep/             BurntSushi/ripgrep fixture (--depth 50).
  db.sqlite                  Created on first server start.
```

### Backend contract (see `libs/lgtm-grpc/proto/lgtm.proto`)

Diff:
- `GetChangedFiles(repo_path, base_ref)` → `files: [{path, status}]`
- `GetFileDiff(repo_path, path, base_ref)` → `{diff_text}`

Identity:
- `UpsertUser(name, kind: AUTHOR_KIND_USER | AUTHOR_KIND_AGENT)` → `{user}`

Comments:
- `ListThreads(repo_path, base_ref, file_path?, only_unresolved?)` → `{threads: [{..., comments: [...]}]}`
- `CreateThread(repo_path, base_ref, file_path, line, author_id, body)` → `{thread}`
- `AppendComment(thread_id, author_id, body)` → `{comment}`
- `ResolveThread(thread_id)` → `{thread}`

## Alternatives

Three distinct frontend tactics are used deliberately:

- **TanStack SPA**: full React + Vite build. Shiki-powered highlighting. Modules split into `lib/`, `components/`, `routes/`, `styles/`.
- **Tauri shell**: single HTML file + vanilla ES-module JS; no bundler, no Shiki. Diff coloring only; CSS and markup mirror the SPA for parity without a shared build pipeline.
- **GPUI**: native widget tree. Per-line colored divs built from `parse_diff` (mirror of the TS classifier).

The agentic path is a separate alternative: the `lgtm` CLI drives the same backend without any UI.

## Experiments

| Approach | Result |
| --- | --- |
| Premise scaffolding | ok |
| Connect-JSON backend (diff + comments) | ok — all 7 endpoints verified |
| TanStack Start + Shiki | ok — 296 highlighted tokens on Cargo.lock, diff lines classified |
| Tauri 2 placeholder + diff coloring | compiles; GUI not launched in this run |
| GPUI two-pane view + colored diff | ok — window renders, lines tinted |
| `lgtm-cli` end-to-end comment lifecycle | ok — create / reply / resolve / list filters all pass |

## Known issues

- GPUI requires full Xcode (`xcrun metal`). The source compiles; CLT alone fails at shader compilation.
- GPUI lives in its own Cargo workspace due to a `toml` version conflict with Tauri 2.11.6.
- GPUI still reads from `stub.rs`; swapping it to call lgtm-grpc is a ~30-line change (`reqwest` is not yet a dep).
- `~/Projects/premise-cargo/premise.yaml` has `field ci not found in type core.Template`; `pm template ls` errors and `pm generate` is TTY-only (bubbletea).

## Learnings

- Rust's "Connect RPC" ecosystem is still immature; hand-written JSON unary over axum is tiny, readable, and wire-compatible with Connect clients.
- One `.proto` as the source of truth + hand-written serde + TS types keeps the three stacks aligned without a codegen pipeline.
- Shiki with a hard-coded `extension → lang` map and a lazy singleton gives passable per-line highlighting without noticeable startup cost; the diff classifier runs over the diff text first and Shiki only touches context + added + removed lines.
- Behavioral parity across the three apps is maintained by naming + palette (shared `lgtm-*` class names and the same hex tokens), not by a shared bundle.
- Agents authenticate implicitly via `--as <name> --as-kind agent`; the server upserts into `users` on first use. Good enough for a PoC; a real deployment would need capability tokens.

## Compare with main at branch creation

The `base_commit` is the full SHA of `origin/main` when this branch was created; do not update it when main moves. Compare this PoC with its starting point using `git diff <base_commit> HEAD`.

## Disposition

Freeze with an annotated tag once the GPUI stub swap lands (or declare it out of scope and freeze now). Do not merge this branch. Follow-up PoCs: in-app comment UIs; side-by-side / split-pane diff; keyboard nav; PR integration.
