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
- **GPUI (`apps/lgtm-desktop-gpui`)**: native three-pane view against the live backend, built on `gpui-kit` 0.7 (the `gpui-pre` fork of GPUI plus the `gpui-component` widgets). Colored diff, syntect syntax highlighting (highlighted once per file selection, then cached), inline thread cards + threads sidebar. **Create, reply and resolve all happen in the GPUI window**: the "+" on a diff line and "Reply" on a thread open an inline `Textarea` composer (Cmd+Enter or Submit posts, Esc or Cancel closes, Enter is a newline). The view is three entities (`ReviewView`, `FileTree`, `DiffView`) so composer keystrokes do not re-render the file list or sidebar, and the diff is a virtualized `list` so only rows near the viewport are built and painted.
- **E2E (`e2e/{common,grpc,cli,web}`)**: 8 grpc + 5 cli + 3 web (Playwright) = **16 tests** covering file listing, diffs, user upsert, create/reply/resolve, filters, error paths, and SPA/Tauri rendering. Shared fixtures in `e2e/common` keep each test declarative.

## Setup and tasks

Required tools:

- `rust` 1.89+ (observed with 1.98.1). The root workspace declares 1.88, but the GPUI workspace's dependencies (`serde-saphyr`, `smol_str`) refuse to build below 1.89.
- `bun` 1.4.2
- `pm` / `premise` CLI
- **GPUI only**: full Xcode (`xcode-select -s /Applications/Xcode.app/Contents/Developer` + `xcodebuild -license accept`)
- **E2E web only**: Playwright chromium-headless-shell (`cd e2e/web && bunx playwright install chromium-headless-shell`)

Workspace layout:
- Rust Cargo workspace at the repo root: `apps/lgtm-{cli,desktop-tauri,server}`, `libs/lgtm-{grpc,db-client}`, `e2e/{common,grpc,cli}`.
- `apps/lgtm-desktop-gpui` is a **standalone** workspace (its own `Cargo.lock`) because the GPUI crate graph needs a newer `toml` (`^0.8.8`, via cbindgen) than Tauri allows: Tauri 2.11.6 pins `toml 0.8.2` via `webkit2gtk → gdk-sys → system-deps`.
- `apps/lgtm-web-tanstack-spa` and `e2e/web` are standalone bun packages.
- `.tmp/` is gitignored: holds `db.sqlite` + `repos/<fixture>`.

| Task | Command | Observed result (2026-10-10 unless noted) |
| --- | --- | --- |
| Build (fan-out, excl. GPUI) | `pm run //...:build` | ok |
| Test (fan-out, excl. GPUI) | `pm run //...:test` | ok |
| Build workspace + web | `mise run lgtm:build` | ok |
| Build GPUI (debug) | `mise run lgtm:build:gpui` | ok (needs full Xcode), 6 dead-code warnings |
| Build GPUI (release) | `mise run lgtm:build:gpui:release` | ok — 27 MB binary |
| Serve backend | `mise run lgtm:serve` | not re-run — an already-running server on `127.0.0.1:50051` served the GPUI smoke run and the e2e suites spawn their own |
| Dev web + backend | `mise run lgtm:dev:web` | not re-run (ok on 2026-10-09) |
| Run Tauri | `mise run lgtm:run:tauri` | not re-run (ok on 2026-10-09) |
| Run GPUI (debug) | `mise run lgtm:run:gpui` | ok — window opens, loads 6 files and a 211-line diff from the backend |
| Run GPUI (release) | `mise run lgtm:run:gpui:release` | not run on 2026-10-10 (release build only) |
| CLI | `mise run lgtm:cli -- <args>` | not re-run directly; the 5 cli e2e tests drive the same binary |
| grpc e2e | `cargo test -p lgtm-e2e-grpc` | **8/8 pass** |
| cli e2e | `cargo test -p lgtm-e2e-cli` | **5/5 pass** |
| web e2e | `cd e2e/web && bunx playwright test` | **3/3 pass** |

There are no automated tests for the GPUI app; it is validated by building it and running it against the backend. `cargo fmt --check` is not a configured task and reports drift in six files of the GPUI crate.

GPUI debug knobs: `LGTM_TRACE_RENDER=1` logs a line per render of each of the three entities; `LGTM_AUTO_COMPOSER=1` opens a composer on line 1 as soon as a diff loads, for typing benchmarks without clicking "+".

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

### Re-verification (2026-10-10, after the GPUI composer and perf work)

- `pm run //...:build`, `pm run //...:test`, `mise run lgtm:build`, both GPUI builds: ok.
- Rust e2e 13/13 and Playwright 3/3 pass.
- `LGTM_TRACE_RENDER=1 mise run lgtm:run:gpui` against a running backend: the window opened and logged `FileTree render … files=6` and `DiffView render … lines=211`.
- **Not re-verified on this date**: the Tauri window, the web dev server, and interactive use of the GPUI composer (posting a comment, scrolling the virtualized list). Those were exercised by hand during development, not by an automated check.

## Performance profile

Measured on the host machine (macOS, Apple Silicon), release builds, backend running against the ripgrep fixture.

| App | Cold launch (window / HTTP ready) | Idle RSS (parent) | Binary / bundle | Notes |
| --- | --- | --- | --- | --- |
| `lgtm-server` | 566 ms (first HTTP 200) | 6.8 MB | 6.0 MB binary | Instant steady state, SQLite open + axum listen |
| SPA dev server (vite + bun) | 1 161 ms (first HTTP 200) | 3.8 MB | 11 MB client bundle (dominated by Shiki grammars; code-splittable) | Not comparable to Tauri/GPUI — this is just the dev server, not the browser |
| Tauri desktop | 546 ms to window visible | 83 MB parent + system WebKit XPCs (3 helpers) | 9.3 MB Rust binary + browser rendering | Loads the SPA via `devUrl`; webview inherits WebKit cache behaviour |
| GPUI desktop | 408 ms to window visible | 63.6 MB (single process) | 12 MB binary (self-contained renderer) | Native Metal renderer; no system webview. **Measured on 2026-10-09 with Zed's git `gpui`, before the move to `gpui-kit`; not re-measured.** The release binary is now 27 MB. |

Backend latencies against the ripgrep fixture (local loopback, 5 samples):
- `GetChangedFiles`: 8.6, 9.3, 9.8, 9.6, 31.6 ms (first call is cold)
- `GetFileDiff`: 6.8, 6.0, 7.1, 7.2, 6.3 ms

### GPUI typing and scroll responsiveness

Typing in the composer lagged badly at first. What was measured, on the 211-line `Cargo.lock` diff with `LGTM_TRACE_RENDER=1`:

- One `ReviewView` entity: 50 keystrokes caused about 48 full-view renders.
- After splitting into `ReviewView` / `FileTree` / `DiffView`: 30 keystrokes caused 1 render of each.
- Release build with the split: 60 keystrokes caused zero `DiffView` re-renders.

The changes behind that, in order: cache syntect spans per file selection instead of highlighting on every render; drop the `subscribe_in` on the `Textarea` (it re-rendered the subscriber on every keystroke) and listen for its `Enter` / `Escape` actions instead; split the view into three entities; virtualize the diff with `list`; build dependencies at `opt-level = 3` in the dev profile.

The last two (virtualized list, optimized dev dependencies) have **no recorded measurements** — they were judged by feel. The debug build still re-renders all three entities without any input (17 renders each in a 12-second idle smoke run on 2026-10-10), so the release build remains the one to use day to day.

### How to read these numbers

- **GPUI** has the smallest window-visible cold launch and the smallest per-process memory, with a self-contained binary — no runtime WebView. It is the best fit for a hyper-responsive native experience; the cost is maintaining a second rendering code path, GPUI's pre-release API churn, and hand-tuning render invalidation to keep typing smooth (see above).
- **Tauri** reuses the SPA code with ~140 ms more cold launch than GPUI and >1.3× the parent RSS before counting WebKit helpers. For a stack built around web tech, it is the lowest-friction native wrapper.
- **SPA** (served to any browser) is the easiest to deploy and iterate — Shiki + React UI "just works" everywhere. The 11 MB client bundle is a Shiki artifact and can be code-split per language.
- The **server + CLI** are shared across all three; comment threads created in the CLI show up live in every UI on refresh (verified by Playwright `CLI-created thread shows up in the SPA on refresh`).

Suggested call: ship the TanStack SPA for breadth, package it with Tauri when a desktop app is needed, and keep GPUI as a performance-sensitive native path once GPUI settles its API.

## Code map

```
Cargo.toml                 Rust workspace (apps/lgtm-{cli,desktop-tauri,server}, libs/*, e2e/{common,grpc,cli})
premise.yaml               Workspace manifest
mise.toml                  Workspace mise config + lgtm:{serve,dev:web,run:*,build:*,cli}; GPUI has debug and :release variants
apps/
  lgtm-server/               HTTP server binary that hosts the Connect-JSON service.
    src/main.rs                Boot: open sqlite, bind 127.0.0.1:50051.
  lgtm-cli/                  `lgtm` binary.
    src/main.rs                clap command tree + env defaults.
    src/client.rs              Blocking reqwest wrapper around Connect-JSON.
  lgtm-desktop-gpui/         Standalone workspace; gpui-kit (gpui-pre + gpui-component) + syntect.
    src/main.rs                Platform boot: open window, mount ReviewView.
    src/review.rs              ReviewView: thin three-pane shell; renders the threads sidebar + identity badge.
    src/file_tree.rs           FileTree entity: left pane; tells DiffView which file is selected.
    src/diff_view.rs           DiffView entity: virtualized diff list, inline thread cards, Textarea composer, create/reply/resolve.
    src/review_state.rs        Env-driven config, ComposerTarget, CachedDiffLine.
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

- **GPUI debug builds are sluggish.** Even with optimized dependencies the debug build keeps re-rendering all three entities while idle; use `mise run lgtm:run:gpui:release` for real use.
- **GPUI's `LGTM_REPO` fallback is a hardcoded absolute path** to this worktree's `.tmp/repos/ripgrep` (`src/review_state.rs`). It only works on the original machine; `mise run` sets `LGTM_REPO`, so the tasks are unaffected.
- **GPUI leftovers from the entity split**: `ReviewState` still carries diff/thread fields and two methods that nothing reads (the 6 dead-code warnings), `set_selected_file` contains an `LGTM_AUTO_COMPOSER` timer that does nothing (the working hook is in `render`), and the palette constants are duplicated across `review.rs`, `diff_view.rs` and `file_tree.rs`.
- **`diff_view.rs` is the largest file in the PoC (752 lines)** and mixes loading, composer state and all row rendering; it is the first thing to split in a follow-up.
- **GPUI has no automated tests** and its `rust-version = "1.88"` is stale (its dependencies need 1.89).
- **GPUI requires full Xcode** (`xcrun metal`). The source compiles; CLT alone fails at shader compilation.
- **GPUI lives in its own Cargo workspace** due to a `toml` version conflict with Tauri 2.11.6. The comment in the root `Cargo.toml` still refers to Zed's crate graph.
- `~/Projects/premise-cargo/premise.yaml` has `field ci not found in type core.Template`; `pm template ls` errors and `pm generate` is TTY-only (bubbletea).

## Learnings

- Rust's "Connect RPC" ecosystem is still immature; hand-written JSON unary over axum is tiny, readable, and wire-compatible with Connect clients.
- One `.proto` as the source of truth + hand-written serde types in Rust/TS keeps the three stacks aligned without a codegen pipeline. The GPUI app duplicates these types in its own `src/proto.rs` because it's in a separate workspace.
- Pointing Tauri at the SPA's `devUrl` collapses two desktop implementations into one; everything we build in the SPA (Shiki, inline threads, keyboard nav) shows up in Tauri with zero duplicate code.
- `syntect` + default theme is a drop-in syntax highlighter for GPUI; no asset shipping. It must be run once per file and cached — highlighting inside `render` made every keystroke re-highlight the whole diff.
- Raw GPUI has no usable text input outside Zed's `editor` crate. A hand-rolled key handler got as far as a blinking caret; switching to `gpui-kit` gave a real `Textarea` (selection, clipboard, IME) with almost no change to the rendering code, because `use gpui_kit::*` is GPUI.
- In GPUI, render cost follows entity boundaries. Subscribing a large view to a `Textarea`'s events re-rendered that view per keystroke; putting the composer's owner in its own entity and handling the `Textarea`'s bubbled `Enter` / `Escape` actions (its own bindings win over outer key contexts) kept keystrokes local.
- A virtualized `list` caches the height of off-screen rows, so anything that changes a row's height (a thread card or composer appearing) has to ask for that row to be re-measured. Keeping a line's threads and composer inside that line's list item keeps item indices stable.
- Unknown: how much the virtualized list and `opt-level = 3` dev dependencies each buy — neither was measured, and both were only tried on a 211-line diff.
- Agents authenticate implicitly via `--as <name> --as-kind agent`; the server upserts into `users` on first use.
- E2E tests feel declarative when the fixtures crate hides port picking / db isolation / server spawn. The whole `TestEnv` abstraction is <200 LoC and makes each test a 10-line scenario.

## Compare with main at branch creation

The `base_commit` is the full SHA of `origin/main` when this branch was created; do not update it when main moves. Compare this PoC with its starting point using `git diff <base_commit> HEAD`.

## Disposition

Freeze with an annotated tag. Do not merge this branch. Follow-up PoCs: measure GPUI scroll/typing on a multi-thousand-line diff; split `diff_view.rs`; keyboard nav; side-by-side diff; PR integration; code-splitting Shiki per language to shrink the SPA bundle.
