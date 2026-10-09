---
base_commit: '386919a'
---

# PoC: lgtm — three-stack code review on one Connect-JSON service

> Experimental branch only. Do not merge into production or publish as a release.

## Purpose

Can three UI stacks — GPUI (native Rust), Tauri (webkit-wrapped Rust shell loading the web app), and TanStack Start (React SPA on Bun) — present the same minimal code-review UI backed by a single Rust Connect-JSON service and an agent-friendly CLI?

Success criteria:
- The three apps show the same list of changed files + colored unified diff + syntax-highlighted code + per-line comment threads for a given OSS fixture repo.
- A `lgtm` CLI drives the same backend (list files, print diffs, create/reply/resolve threads) under a user **or** agent identity.
- 16 end-to-end tests across the server, CLI, and web app cover the agentic + human review flows.

## Current status

- **Server (`apps/lgtm-server`)**: Connect-JSON HTTP, 7 unary endpoints.
- **DB (`libs/lgtm-db-client`)**: SQLite — `users`, `threads`, `comments` tables.
- **CLI (`apps/lgtm-cli`)**: `lgtm {files,diff,user upsert,threads,comment {add,reply,resolve}}`.
- **SPA (`apps/lgtm-web-tanstack-spa`)**: three-pane layout, Shiki syntax highlighting, inline + sidebar comment threads, identity badge, create/reply/resolve.
- **Tauri (`apps/lgtm-desktop-tauri`)**: wraps the SPA via `devUrl=http://localhost:3000` + `frontendDist=../lgtm-web-tanstack-spa/dist/client`. One frontend codebase; Tauri inherits every SPA feature.
- **GPUI (`apps/lgtm-desktop-gpui`)**: native three-pane view against the live backend. Colored diff, syntect syntax highlighting, read-only threads inline + in sidebar, `Resolve` button that calls the API, "Copy reply cmd" / "+" buttons that put a prefilled `lgtm comment add/reply` command on the clipboard. **GPUI thread create/reply go through the CLI** (see Known issues).
- **E2E (`e2e/{common,grpc,cli,web}`)**: 8 grpc + 5 cli + 3 web (Playwright) = **16 tests** covering file listing, diffs, user upsert, create/reply/resolve, filters, error paths, and SPA/Tauri rendering. Shared fixtures in `e2e/common` keep each test declarative.

## Setup and tasks

Required tools:

- `rust` 1.88+
- `bun` 1.4.2
- `pm` / `premise` CLI
- **GPUI only**: full Xcode (`xcode-select -s /Applications/Xcode.app/Contents/Developer` + `xcodebuild -license accept`)
- **E2E web only**: Playwright chromium-headless-shell (`cd e2e/web && bunx playwright install chromium-headless-shell`)

Workspace layout:
- Rust Cargo workspace at the repo root: `apps/lgtm-{cli,desktop-tauri,server}`, `libs/lgtm-{grpc,db-client}`, `e2e/{common,grpc,cli}`.
- `apps/lgtm-desktop-gpui` is a **standalone** workspace (its own `Cargo.lock`) because Zed's gpui pins `toml ^0.8.8` via cbindgen, while Tauri 2.11.6 pins `toml 0.8.2` via `webkit2gtk → gdk-sys → system-deps`.
- `apps/lgtm-web-tanstack-spa` and `e2e/web` are standalone bun packages.
- `.tmp/` is gitignored: holds `db.sqlite` + `repos/<fixture>`.

| Task | Command | Observed result (2026-10-09) |
| --- | --- | --- |
| Build (fan-out, excl. GPUI) | `pm run //...:build` | ok |
| Test (fan-out, excl. GPUI) | `pm run //...:test` | ok |
| Build server | `cargo build -p lgtm-server` | ok |
| Build CLI | `cargo build -p lgtm-cli` | ok |
| Build web | `mise run //apps/lgtm-web-tanstack-spa:build` | ok |
| Build Tauri | `cargo build -p lgtm-desktop-tauri` | ok |
| Build GPUI | `mise run lgtm:build:gpui` | ok (needs full Xcode) |
| Serve backend | `mise run lgtm:serve` | ok — listens on `127.0.0.1:50051` |
| Dev web + backend | `mise run lgtm:dev:web` | ok — Ctrl-C stops both |
| Run Tauri | `mise run lgtm:run:tauri` | ok — `cargo tauri dev` auto-launches the SPA dev server via `beforeDevCommand` |
| Run GPUI | `mise run lgtm:run:gpui` | ok — connects to the backend on launch |
| CLI | `mise run lgtm:cli -- <args>` | ok |
| grpc e2e | `cargo test -p lgtm-e2e-grpc` | **8/8 pass** |
| cli e2e | `cargo test -p lgtm-e2e-cli` | **5/5 pass** |
| web e2e | `cd e2e/web && bunx playwright test` | **3/3 pass** |

### CLI smoke (reproducible)

```
mise run lgtm:serve  &
./target/debug/lgtm files .tmp/repos/ripgrep --base HEAD~5
./target/debug/lgtm diff  .tmp/repos/ripgrep --base HEAD~5 --path crates/ignore/src/gitignore.rs

tid=$(./target/debug/lgtm comment add .tmp/repos/ripgrep \
  --path crates/ignore/src/gitignore.rs --line 361 --base HEAD~5 \
  --body "nit: pool capacity change could regress at 1 thread" \
  --as claude-code --as-kind agent)
./target/debug/lgtm comment reply --thread $tid --body "benchmarked; no regression; LGTM" --as sid --as-kind user
./target/debug/lgtm threads .tmp/repos/ripgrep --base HEAD~5
./target/debug/lgtm comment resolve --thread $tid
```

### Autonomous verification evidence (2026-10-09)

- **Playwright** (`e2e/web/tests/review.spec.ts`): file-list rendering, Shiki tokens on the diff, create thread via "+ button" → reply → resolve, CLI-created thread appearing in the SPA on refresh. 3/3 pass.
- **Rust e2e** (`e2e/grpc`, `e2e/cli`): server endpoints + CLI subcommands + Connect error envelope + idempotent upsert + file-scoped thread filtering. 13/13 pass.
- **Headless Chrome DOM dump**: 6 file-list items; **296 Shiki tokens** on Cargo.lock; `lgtm-line-*` classes on all diff rows.
- **macOS `screencapture`**: Tauri window shows the SPA's three-pane layout (confirmed `lgtm-desktop-tauri` title + Shiki tokens + threads sidebar); GPUI window shows the colored diff + syntect-highlighted TOML tokens.

## Performance profile

Measured on the host machine (macOS, Apple Silicon), release builds, backend running against the ripgrep fixture.

| App | Cold launch (window / HTTP ready) | Idle RSS (parent) | Binary / bundle | Notes |
| --- | --- | --- | --- | --- |
| `lgtm-server` | 566 ms (first HTTP 200) | 6.8 MB | 6.0 MB binary | Instant steady state, SQLite open + axum listen |
| SPA dev server (vite + bun) | 1 161 ms (first HTTP 200) | 3.8 MB | 11 MB client bundle (dominated by Shiki grammars; code-splittable) | Not comparable to Tauri/GPUI — this is just the dev server, not the browser |
| Tauri desktop | 546 ms to window visible | 83 MB parent + system WebKit XPCs (3 helpers) | 9.3 MB Rust binary + browser rendering | Loads the SPA via `devUrl`; webview inherits WebKit cache behaviour |
| GPUI desktop | 408 ms to window visible | 63.6 MB (single process) | 12 MB binary (self-contained renderer) | Native Metal renderer; no system webview |

Backend latencies against the ripgrep fixture (local loopback, 5 samples):
- `GetChangedFiles`: 8.6, 9.3, 9.8, 9.6, 31.6 ms (first call is cold)
- `GetFileDiff`: 6.8, 6.0, 7.1, 7.2, 6.3 ms

### How to read these numbers

- **GPUI** has the smallest window-visible cold launch and the smallest per-process memory, with a self-contained binary — no runtime WebView. It is the best fit for a hyper-responsive native experience; the cost is maintaining a second rendering code path and GPUI's pre-release API churn.
- **Tauri** reuses the SPA code with ~140 ms more cold launch than GPUI and >1.3× the parent RSS before counting WebKit helpers. For a stack built around web tech, it is the lowest-friction native wrapper.
- **SPA** (served to any browser) is the easiest to deploy and iterate — Shiki + React UI "just works" everywhere. The 11 MB client bundle is a Shiki artifact and can be code-split per language.
- The **server + CLI** are shared across all three; comment threads created in the CLI show up live in every UI on refresh (verified by Playwright `CLI-created thread shows up in the SPA on refresh`).

Suggested call: ship the TanStack SPA for breadth, package it with Tauri when a desktop app is needed, and keep GPUI as a performance-sensitive native path once Zed's gpui settles its API.

## Code map

```
Cargo.toml                 Rust workspace (apps/lgtm-{cli,desktop-tauri,server}, libs/*, e2e/{common,grpc,cli})
premise.yaml               Workspace manifest
mise.toml                  Workspace mise config + lgtm:{serve,dev:web,run:*,build:*,cli}
apps/
  lgtm-server/               HTTP server binary that hosts the Connect-JSON service.
    src/main.rs                Boot: open sqlite, bind 127.0.0.1:50051.
  lgtm-cli/                  `lgtm` binary.
    src/main.rs                clap command tree + env defaults.
    src/client.rs              Blocking reqwest wrapper around Connect-JSON.
  lgtm-desktop-gpui/         Standalone workspace; Zed gpui + gpui_platform + syntect.
    src/main.rs                Platform boot: open window, mount ReviewView.
    src/review.rs              Three-pane view (file list, diff + inline threads, threads sidebar).
    src/review_state.rs        State owned by ReviewView; env-driven config.
    src/client.rs              Blocking HTTP (ureq) client for Connect-JSON.
    src/proto.rs               Serde types mirroring libs/lgtm-grpc/proto/lgtm.proto.
    src/diff.rs                Diff classifier + per-line new-line numbering.
    src/highlight.rs           Lazy syntect highlighter.
  lgtm-desktop-tauri/        Tauri 2 Rust shell that wraps the SPA.
    tauri.conf.json            devUrl=http://localhost:3000; frontendDist=../lgtm-web-tanstack-spa/dist/client.
  lgtm-web-tanstack-spa/     TanStack Start SPA (bun + vite). Powers Tauri too.
    src/lib/lgtm-client.ts     Connect-JSON client (all 7 RPCs).
    src/lib/identity.ts        localStorage-backed identity resolution.
    src/lib/review-state.ts    Composite hook: files/selected/diff/threads/user + verbs.
    src/lib/diff-lines.ts      Unified-diff classifier (parity with GPUI's src/diff.rs).
    src/lib/highlight.ts       Lazy Shiki singleton + extension→lang mapping.
    src/components/ReviewLayout.tsx  Three-pane layout; computes new-file line numbers.
    src/components/ThreadView.tsx    Thread card: comments, reply composer, Resolve.
    src/components/NewThreadComposer.tsx  "+" button inline composer.
    src/styles/review.css      Palette + layout.
    src/routes/index.tsx       Route; URL params ?repo=, ?base=, ?backend=.
libs/
  lgtm-grpc/                 Backend library — Connect-JSON unary service.
    proto/lgtm.proto           Source of truth. Hand-mirrored in src/proto.rs + apps/lgtm-desktop-gpui/src/proto.rs.
    src/proto.rs               Serde messages.
    src/git.rs                 git2 primitives.
    src/service.rs             LgtmService trait + DefaultService (holds Arc<Db>).
    src/server.rs              axum Router: 7 POST routes under /lgtm.v1.LgtmService.
    src/error.rs               ServiceError + Connect error envelope.
  lgtm-db-client/            SQLite wrapper.
    src/db.rs                  Db::{upsert_user, create_thread, append_comment, resolve_thread, list_threads}.
    src/schema.rs              Idempotent init SQL (users, threads, comments; legacy reviews table kept).
e2e/
  common/                    Shared fixtures — spawn-per-test `TestEnv` with isolated port + db + git repo.
  grpc/                      Rust integration tests of the HTTP endpoints.
  cli/                       Rust integration tests of the `lgtm` binary via assert_cmd.
  web/                       Playwright tests of the SPA (and therefore Tauri).
.tmp/                      gitignored
  repos/ripgrep/             BurntSushi/ripgrep fixture (--depth 50).
  db.sqlite                  Created on first server start (overridable via LGTM_DB_PATH).
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

## Known issues

- **GPUI comment create/reply go through the CLI, not the GPUI UI.** GPUI's text-input story is tied to the full `editor` crate from Zed's workspace, which is too heavy for a PoC. The GPUI UI shows threads read-only inline + in the sidebar, has a working `Resolve` button (API call, no text needed), and a "+ / Copy reply cmd" button next to each line that copies a prefilled `lgtm comment add/reply` command to the clipboard. The reviewer runs it in a terminal to post the comment, then the GPUI window picks it up on reload. This is a deliberate scope cut; a full in-GPUI text editor is a follow-up.
- **GPUI requires full Xcode** (`xcrun metal`). The source compiles; CLT alone fails at shader compilation.
- **GPUI lives in its own Cargo workspace** due to a `toml` version conflict with Tauri 2.11.6.
- `~/Projects/premise-cargo/premise.yaml` has `field ci not found in type core.Template`; `pm template ls` errors and `pm generate` is TTY-only (bubbletea).

## Learnings

- Rust's "Connect RPC" ecosystem is still immature; hand-written JSON unary over axum is tiny, readable, and wire-compatible with Connect clients.
- One `.proto` as the source of truth + hand-written serde types in Rust/TS keeps the three stacks aligned without a codegen pipeline. The GPUI app duplicates these types in its own `src/proto.rs` because it's in a separate workspace.
- Pointing Tauri at the SPA's `devUrl` collapses two desktop implementations into one; everything we build in the SPA (Shiki, inline threads, keyboard nav) shows up in Tauri with zero duplicate code.
- `syntect` + default theme is a drop-in syntax highlighter for GPUI; no asset shipping.
- Agents authenticate implicitly via `--as <name> --as-kind agent`; the server upserts into `users` on first use.
- E2E tests feel declarative when the fixtures crate hides port picking / db isolation / server spawn. The whole `TestEnv` abstraction is <200 LoC and makes each test a 10-line scenario.

## Compare with main at branch creation

The `base_commit` is the full SHA of `origin/main` when this branch was created; do not update it when main moves. Compare this PoC with its starting point using `git diff <base_commit> HEAD`.

## Disposition

Freeze with an annotated tag. Do not merge this branch. Follow-up PoCs: in-GPUI text input (via a trimmed-down editor or home-grown input widget); keyboard nav; side-by-side diff; PR integration; code-splitting Shiki per language to shrink the SPA bundle.
