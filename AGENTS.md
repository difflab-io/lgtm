# LGTM implementation conventions

## Repo Structure

- Keep backend domain models in `apps/lgtm-backend/src/models/`, services in `src/services/`, and application errors in `src/error.rs`.
- Put general helpers in `src/utils.rs`. Keep forge import contracts in `src/forge/models.rs` and review export behavior with its domain model. Domain models represent application entities.
- Keep database entities private; expose actual SeaORM models and the repositories required by backend workflows. Shared SeaORM entity types belong in `libs/lgtm-db/src/entities/common.rs`.

## Docs

- Use soft wraps for documentation prose. Keep each paragraph on one source line and let the editor wrap it for display. Do not add manual line breaks between sentences or at a fixed column width. Preserve structural line breaks in lists, tables, code, and Mermaid diagrams.
- Embed architecture diagrams as inline Mermaid fences so MmdToImg can preview the source in Neovim. Keep diagram source in the Markdown page, without separate diagram directories or tracked preview images.

- [Proposed] Put component public APIs and state/data flow under Design. Keep Implementation to brief, non-obvious maintainer notes; do not repeat parent architecture docs.

## Code Style

- During alpha, update branch-introduced contracts directly. Do not retain duplicate fields or add compatibility code for earlier versions of this branch.

- Keep stateless SDK and CLI adapters in `apps/lgtm-backend/src/extensions/` (for example, `extensions/gh.rs`). Call existing library features directly and share external-tool wrappers here instead of duplicating them in services or providers. Keep domain mapping and review behavior in their existing model and service modules.
- Name mutating operations and factories with imperative verbs (for example, `promote`, not `promoted`).

- Name public repository and service reads `get_*`.
- Use one inherent `impl` block per type per file. Put constructors and configuration methods first, then public operations, then private helpers. Put required trait implementations after the inherent block. Add `// Public` and `// Private` section headers, padded with hyphens to column 80, and use logical subsection headers such as `// Sessions`, `// Threads` and `// Comments` to group related methods.
- Keep one main model type per file where practical. Keep validators and `from_*` / `to_*` / `into_*` conversions beside that type; RPC modules contain transport handlers, not model conversion functions.
- Separate functions with a blank line. Keep unit tests last, after a `// tests ---` separator padded to column 80.
- Use `// fixtures ---` and `// tests ---` separators padded to column 80 in test files.
- Call standard/library functions directly. Do not add aliases that only forward one method call.
- Document entity purpose, field scope, state semantics and public contracts. Omit filler and redundant implementation narration.
- Name fields that contain complete URLs with a `_url` suffix. Do not label hostnames, filesystem paths, repository keys or opaque forge IDs as URLs.
- Use `uuid::Uuid` for internal IDs and foreign keys. Keep external forge IDs as opaque scoped strings; name forge repository addresses `repository_url` and local Git directories `git_dir`.
- Model contexts as `Local` with a notes/branch location or `ForgeBacked` with a validated repository URL and PR/MR number. Reject remote operations for local contexts.
- Use enums for discriminators and state-dependent data; enforce essential shape rules with SQL constraints.

## Test Organization

These rules apply to every app and library in the workspace.

- Colocate private unit tests in the source file under `#[cfg(test)] mod tests`. Shared unit fixtures belong under `src/tests/`.
- Use separate integration test files directly under `tests/` for each service or repository area (for example, `tests/review_service.rs`). Group cases with inline `mod name { ... }` blocks inside those files. Do not consolidate all integration tests into one file or add a nested `tests/integration/` directory or `#[path]` declarations. Shared integration fixtures belong under `tests/common/`; do not use `tests/support/` or `tests/test_support/`.
- Name Rust test functions `should_<behavior>` and keep fixture helpers separate from test cases with the standard separators.
- [Proposed] Use human-readable BDD titles in Vitest and Lua tests. Group cases under nested `When ...` contexts, and separate fixtures, unit tests and integration tests with file section dividers.
- [Proposed] Test plugin source switching through the root Mise tasks. Limit installation fixtures to isolated Neovim profiles and cleanup; do not recreate the installer or construct substitute plugin repositories.
- Test multi-operation transactions and full review lifecycles against real SQLite and PostgreSQL. Use dummy external services.
- Run checks through project Mise tasks invoked by Premise. Keep their source scripts tracked; generated output stays ignored.
- Use `e2e/README.md` for PR E2E replay, recording refresh and live parity commands. Keep TypeScript E2E tests under e2e/, outside Rust app test directories.
- During development, run only tests relevant to the changed behavior, feature or suite. Use `premise run e2e:smoke` for a quick check across features; use a spec path, test name or Vitest tag for narrower checks. Do not run the full E2E suite on each iteration. Reserve a full run for final validation or changes to shared fixtures or behavior that require all suites, and reuse passing evidence for unchanged code.
- Write new E2E suites in TypeScript with Vitest. Use native Vitest `describe` and `it` groups for feature contexts and independent lifecycle journeys, with explicit assertions and shared runtime fixtures. Keep notes, local reviews, PR reviews and diffs in separate spec files where those boundaries exist; import Vitest `it` directly and use shared `beforeEach`/`afterEach` fixtures instead of custom step runners or test aliases. Keep each suite's Mise config; do not add a root `e2e/mise.toml`.
- Use the permanent private GitHub fixture selected by `LGTM_E2E_GITHUB_REPOSITORY`. Test runners must never create, recreate, rename, archive or delete repositories. Verify private visibility before writes and clean up only run-owned PRs and refs.
- Visually separate Arrange, Act and Assert blocks in E2E tests with section comments. Repeat the sections within long journeys when the next action needs another check.
- Organize E2E specs by top-level `When ...` states and native `it` cases for individual observable behaviors. Put suite and feature labels in Vitest tags. A state group may share run-owned PRs through `beforeAll`/`afterAll`; reset relevant remote feedback between independent cases. Keep credentials scoped to test processes and never switch the user’s active GitHub account during tests.
- Keep shared process and GitHub lifecycle fixtures in `e2e/common/`. Compose editor lifecycle hooks once under `e2e/nvim/`. Keep feature setup in adjacent `*.fixtures.ts` files, and keep small single-file helpers private in their spec. Export only helpers that another module uses.

## Lua and Neovim UI

- Keep review business operations, anchor evidence capture, anchor placement and location-status decisions in backend APIs shared with the web UI. Do not duplicate those rules in Lua; add a backend contract when one is missing. Preserve CodeDiff's native explorer, buffers, editing and refresh experience. CodeDiff and Neovim may read Git and filesystem content directly for that experience. Use backend data wherever it supports the existing experience, and keep editor buffer checks, selection, interaction state and rendering in Lua.
- Keep saved-source move detection and reconciliation in the backend. Editor rename events only request refreshed data; Lua must not retain old/new path associations, pending moves, or move-specific retries.
- Services own reactive application data and expose operations. Components render that data declaratively and call service methods. Compose thread panes from threads, and threads from comment components that receive data and callbacks as inputs. Keep transient selection and window state in components. Do not coordinate data refreshes or push display records through imperative feature controllers.
- [Proposed] Use `app.lua` for plugin orchestration. Keep notes data and export reads in services; reserve `features/` for LGTM-specific UI and `ui/` for reusable widgets.
- Capture anchor evidence from backend-read Git or saved-file content. Clients supply selection intent, not source bodies or evidence.
- Keep component state limited to UI interaction, such as selection, expansion and window visibility. Keep substantive Lua implementation under `features/`, `models/`, `services/` and `ui/`; root modules should be small plugin entry points, except `app.lua`, which owns application orchestration.

- Put review-flow and thread components under `lua/lgtm/features/`, composed around new-review dialogs, branch/PR selection, thread panes, threads and comments. Keep reusable visual components under `ui/`.
- Group Lua RPC clients behind session, thread, file, diff and forge service modules. Keep business workflows in the backend and feature interactions in `features/`.

- Keep shared Lua model declarations under `lua/lgtm/models/`, grouped by model where useful. Keep state management and service operations outside model files.

- Order Lua modules as State, Render, UI, Lifecycle, Methods, then Private/helpers. Use only sections that contain code. Pad `-- Section` separators with hyphens to column 80. Keep public entry points above module-private helpers.
- Put private helper definitions at the bottom. Use a module-local namespace or explicit local forward declarations when earlier functions need them. Do not export helpers only to avoid Lua's declaration order.
- Make each render method reveal the visual structure directly. Compose named sections such as header, body, and footer, or meaningful child components. Keep label calculation, text fitting, wrapping, and annotation conversion outside the composition. Do not hide a large procedure behind a renamed body helper.
- Denote subcomponents with a module-local `UI` namespace, including imported child renderers: `UI.header`, `UI.body`, `UI.footer`, `UI.comment`, or another meaningful component name. Reserve `Private` for non-rendering helpers so render composition is distinct from calculations and workflows.
- Build floating views with NuiComponents composition, props, signals, and lifecycle hooks. Use NUI primitives for persistent splits and source annotations. Reuse documented library behavior before writing imperative window code or a generic component framework. Keep the CodeDiff virtual-line adapter small and separate from card composition.
- Put each component property on its own line. Expand nested property tables. Keep callbacks short and delegate named operations instead of nesting workflow logic in props.
- Keep RPC calls and business workflows under `lua/lgtm/services/`. Group the reads, guarded writes, retries, and state updates needed for a complete operation. UI handlers supply intent and present results. Services must not import UI components or notification providers.
- Reuse the shared review, file, conversation, comment, thread, and draft shapes. Keep shared application state in services and transient interaction state in components. Do not invent a new data model or response envelope for every screen or callback.
- Treat commands and key bindings as public APIs. Document supported keys, modes, remapping, and permission limits alongside Lua APIs. Show UI states with aligned layouts and state/data flows with inline Mermaid. Omit routine styling and maintainer narration.
- Run StyLua formatting and Selene lint through the plugin's Premise tasks. Complexity checks supplement review of component composition; splitting a procedure into arbitrary helpers does not satisfy these rules.
- Keep each UI component function at cyclomatic complexity 10 or below. The lint task enforces this limit across `lua/lgtm/ui/`. Controller and service violations remain visible as warnings at the same threshold until a separate cleanup removes them. Do not raise the UI limit or add suppressions to avoid decomposition.

## CLI

- Keep each command group in its own file under `apps/lgtm-cli/src/commands/`. Order each file as local types, command argument struct, command callbacks, then private helpers. Keep shared command argument types in `src/models.rs` and transport in `src/rpc_client.rs`.

## Backend

- Models validate, map data and implement domain behavior. They must not query or mutate the database; services own persistence orchestration and relationship loading.
- Keep pure entity factory methods with their domain model, not in services.
- Humans require email; agents require model and provider and have no email. Forge account bindings are separate from model providers.
- Expose database queries and mutations only through named repository methods. The DB client is the single source of truth for the persistence rules encapsulated by those operations. Do not expose raw connections, generic query builders or raw SQL escape hatches to callers.
- Expose only operations needed by agreed backend workflows. Update explicit mutable columns.
- Establish tracing spans at CLI/RPC boundaries. Services use tracing macros and do not store request spans.
- Keep CLI review/session/comment commands as backend RPC clients. Preserve daemon and greeting/health behavior; do not embed review persistence in the CLI.
