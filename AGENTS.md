# LGTM Implementation Conventions

## Hard Rules

- Run builds, formatters, linters, and tests through project [Mise tasks](mise.toml) invoked by Premise (`premise run format`, `premise run lint`, `premise run test`, `premise run e2e:smoke`). Keep task scripts tracked and generated outputs ignored. Keep all Mise path helpers, task definitions, and developer tool management inside [`libs/lgtm-env`](libs/lgtm-env/src/lib.rs) and `[.]mise.toml` files, never inside [`libs/lgtm-config`](libs/lgtm-config/src/lib.rs) or `.lgtm/config.json`.
- Store generated config JSON Schemas under [`schemas/`](schemas/) at the repository root and reference them via versioned jsDelivr tag URLs (`https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v<version>/schemas/...`) rather than writing local schema copies into `.lgtm/`.
- During alpha, update branch-introduced contracts directly without compatibility shims or duplicate fields.
- Expose database queries and mutations only through named repository methods in [`libs/lgtm-db`](libs/lgtm-db/src/lib.rs). Keep SeaORM `Entity` and `ActiveModel` types `pub(crate)`; never expose raw connections, query builders, or raw SQL to callers.
- Models validate, map data, and implement pure domain behavior; services own persistence orchestration, cross-scope configuration merging, transactions, and external I/O.
- Keep [`libs/lgtm-rsutils`](libs/lgtm-rsutils/src/lib.rs) strictly limited to generic, domain-agnostic helper functions and thin wrappers over third-party crates (`gix`, `serde`, `std::fs`); never put LGTM-specific domain rules, home directory paths, repository slug conventions, or custom error enums in `lgtm-rsutils`.
- Give each file and service a single responsibility; extract distinct capabilities into dedicated model and service modules with their own integration suites, combine host services into a single unified host state, and wire event handling and cross-service coordination in application entry points rather than emitting framework events or calling sibling services inside domain services. See [`docs/architecture/desktop/events.md`](docs/architecture/desktop/events.md).
- Treat `LgtmConfig` in [`libs/lgtm-config`](libs/lgtm-config/src/lib.rs) as the single configuration and user-settings contract (`~/.lgtm/config.json` and `<workspace>/.lgtm/config.json`); do not split `LgtmConfig` and `UserSettings` into parallel models or service APIs. Configure preferences by editing `config.json` directly in the editor or by dispatching `LgtmConfigPatch` updates from `CommandMenu` and `CommandPalette` via `config_update`, rather than building a dedicated preferences GUI window.
- Keep CLI review and comment commands as thin RPC clients over `lgtm-forge`. Preserve daemon and health behavior; do not embed review persistence in `lgtm-cli`.
- Establish tracing spans at CLI/RPC boundaries (`#[tracing::instrument]`). Services emit tracing events and never store request spans.
- Preserve pinned Neovim dependencies in the plugin setup, source-switch installer, and E2E harness (including NuiComponents/NUI and optional Kak).
- Preserve the Neovim development and release plugin installation tasks when resetting plugin features.
- Never create, recreate, rename, archive, or delete GitHub repositories in tests. Use `LGTM_E2E_GITHUB_REPOSITORY`, verify private visibility before writes, and clean up only run-owned PRs and refs.

## Organization

- `apps/`: User-facing applications and command-line interfaces.
- `libs/`: Shared domain, configuration, environment, protocol, database, and UI libraries.
- `e2e/`: Cross-tier integration and end-to-end test suites.

- Preserve existing documentation template headings and structure; use `NA/WIP` for unimplemented sections.
- Centralize shared workspace path resolution and desktop executable discovery in [`libs/lgtm-config`](libs/lgtm-config/src/lib.rs) rather than duplicating configuration across `lgtm-cli` and `lgtm-desktop`.
- Place each domain model struct (`Project`, `Worktree`, `Session`, `WorkspaceSessionState`, `PersistedTab`) in its own dedicated file under `src/models/`, keep desktop workspace models and [`WorkspaceService`](libs/lgtm-workspace/src/lib.rs) in [`libs/lgtm-workspace`](libs/lgtm-workspace/src/lib.rs) so [`libs/lgtm-session`](libs/lgtm-session/src/lib.rs) remains a thin session aggregation and persistence layer, and scope [`libs/lgtm-project`](libs/lgtm-project/src/lib.rs) worktrees strictly to LGTM-managed checkouts under `~/.lgtm/worktrees/<project>/<branch-slug>`.
- [Proposed] Group desktop Tauri IPC command handlers by domain under `apps/lgtm-desktop/src/commands/`, keep each command handler as a thin wrapper calling `AppState::run_blocking`, flatten and namespace web components under `apps/lgtm-web/src/lib/components/` (`ProjectEditDialog`, `ProjectIconSelector`, `WorktreeNewDialog`, `WorktreeRemoveDialog`, `FileExplorer`, `LeftDock`, `WidgetPane`, `LeftDockToolbar`, `EditorToolbar`), and group frontend domain and dialog types under `apps/lgtm-web/src/lib/types/`.

## Style

### Rust Layout & Naming

- Document all public types, functions, and methods (`pub fn`) with `///` docstrings (`#![warn(missing_docs, unreachable_pub)]`) that state the type's architectural role and boundary purpose.
- Export only public API types from crate roots (`lib.rs`), formatted one `pub use` item per line.
- Link workspace library crates through `Cargo.toml` dependencies rather than `#[path = ...]` module shims or `#[allow(dead_code)]`, and prefer standard library or crate formatting traits (such as `LowerHex` on digest outputs) over manual formatting loops.
- Use one inherent `impl` block per type per file: constructors first, `// Public` operations next, `// Private` helpers last, and trait `impl` blocks after the inherent block. Pad section headers with hyphens to column 80.
- Name public repository and service reads `get_*`. Name mutating operations and factories with imperative verbs (`promote`, `publish_session`). Name domain model methods with `get_`, `set_`, `update_`, or `apply_` prefixes (`get_worktree`, `set_collapsed`, `update_appearance`, `set_worktree_name`, `set_worktree`, `apply_worktree_removal`) to indicate pure in-memory data operations without side effects.
- Call library and generated client methods directly; do not add pass-through wrapper aliases.
- Prefer a single `new` constructor accepting `Option<...>` parameters over multiple overlapping constructors when callers only need one construction path.
- Use `uuid::Uuid` for internal IDs, `repository_url` (with `_url` suffix only for full URLs) for normalized remote addresses, and `git_dir` for canonical local `.git` directories.
- Humans require `email`; agents require `provider` and `model` and have no email. Model review contexts as `Local` or `ForgeBacked` and reject remote operations on `Local` contexts.

```rust
impl CommentAnchor {
    pub fn new(file_path: &str, start_line: Option<u32>, line: u32, commit_sha: &str) -> Result<Self> {
        // constructor before // Public
    }

    // Public ------------------------------------------------------------------

    pub fn validate_line(blob: Option<&str>, file_path: &str, start_line: Option<u32>, line: u32) -> Result<()> {
        // public operations
    }

    // Private -----------------------------------------------------------------

    fn validate_coordinates(file_path: &str, start_line: Option<u32>, line: u32) -> Result<String> {
        // private helpers
    }
}
```

### CLI View Models

- Transform backend RPC responses into CLI view models under `apps/lgtm-cli/src/models/` using `From` conversions and render terminal/JSON output via `print` methods. Name entity structs without a `View` suffix (`Identity`, `CommentAnchor`, `Comment`, `DiffHunk`, `Review`, `Session`) and name command-response wrappers `*CommandOutput` (`AddCommandOutput`, `ShowReviewCommandOutput`). Keep shared CLI argument types and formatting helpers in `apps/lgtm-cli/src/models/common.rs`.

### Desktop & Helical UI Components

- Document all public TypeScript service classes and methods with `/** ... */` JSDoc comments, and maintain Difflab-template architecture documentation for frontend services under `docs/architecture/`.
- Compose `@cloudvoyant/helical-svelte` primitives directly in `apps/lgtm-web`; build missing reusable primitives and services in `libs/lgtm-ui` per Helical UI conventions (`@ark-ui/svelte`, orthogonal `cva` + `VariantProps` in `styles.ts`, `cn` + semantic Tailwind tokens, and app-agnostic `types.ts`) for upstreaming.
- Style components and shell layouts with Tailwind utilities and `cva` variants; restrict `apps/lgtm-web/src/app.css` to theme imports and global element resets, and extract repeated Svelte markup/class patterns into local `{#snippet}` blocks or subcomponents.
- Style IDE menus, context menus, and tab bars compactly with square item highlights (`rounded-none`), full-row submenu trigger hover targets, neutral surface hover/focus tokens (`--accent`, `--ring`), workspace-relative breadcrumbs, and icon-based diagnostic badges only when counts are non-zero.
- Use Helical UI's `<Pane>` component for pane containers rather than manually binding `useOptionalPaneRegistration` when a standard pane element suffices.
- Implement CodeMirror 6 features (LSP completions, hover tooltips, diagnostics, signature help, and modal editing state) as modular CodeMirror `Extension` values using `Facet`, `StateField`, and `ViewPlugin` rather than imperative DOM or `EditorView` manipulation inside Svelte components.
- [Proposed] Cache loaded directory children per workspace so switching workspaces renders expanded tree branches synchronously in a single frame, and constrain CodeMirror's scroll container (`min-h-0 overflow-hidden` with `.cm-scroller` owning scrolling) while warming the syntax tree (`ensureSyntaxTree`) so syntax highlighting never pops in on scroll.
- [Proposed] Split frontend state into dedicated domain services (`WorkspaceService`, `ProjectService`, `SessionService`, `LspService`, `SettingsService`) exposed via `useService` context rather than pass-through getters on `WorkspaceService`, order TypeScript service files with `// Public ---` and `// Private ---` section headers (private helpers at the bottom), and pair a resizable `LeftDock` (`ProjectsPane` + `WidgetPane`) with a unified bottom bar (`LeftDockToolbar` + `EditorToolbar`) whose dock section tracks `LeftDock` width and highlights active toggles via text contrast (`text-foreground` vs `text-muted-foreground`) without background fills.

### Documentation

- Document all environment variables that affect LGTM behavior in [`docs/environment.md`](docs/environment.md).
- Use soft wraps for Markdown prose (one source line per paragraph). Keep diagrams as inline Mermaid fences and prefer file trees, tables, and short code snippets over long prose.
- Write documentation in plain, concise English (`/simple-english`): use short sentences, active voice, simple tenses, and no bold lead-ins in lists, and rely on Mermaid diagrams and tables rather than long prose paragraphs.
- In architecture `## Requirement` sections, do not prefix bullets with `FR` or `NFR` labels; state `### Functional` requirements as user-facing capabilities and `### Non-Functional` requirements as performance, security, isolation, and reliability characteristics without internal crate or implementation fluff.

## Testing

### Rust BDD Unit & Integration Tests

- Colocate unit tests in `#[cfg(test)] mod tests` after a `// tests ---` separator padded to column 80; place shared unit fixtures in `src/tests/`.
- Put integration suites directly under `tests/<module>.rs` named after the module file they exercise (`tests/git.rs`, `tests/project.rs`, `tests/session.rs`) with shared fixtures in `tests/common/mod.rs` (never `tests/integration/` or `tests/support/`). Keep each test focused on a single behavior (`should_<behavior>`) and cover core module behaviors, edge cases, and failure modes at library integration boundaries to keep E2E suites focused on cross-tier wiring.
- Document test fixtures with visual ASCII layout docstrings, structure integration `mod given_*` and `mod when_*` branches incrementally from empty preconditions (`given_no_session_exists`, `given_no_project_exists`, `given_a_project_exists`) through public API invariants, and capture initial state, final state, and verbose tracing logs as verifiable test artifacts.
- Do not write custom JSON schema validation/parsing logic or unit tests for schema validation and serialization; rely on library crates and test domain behavior through public service boundaries.
- Extract shared test fixtures and JSON or text templates into top-level constants and helper functions rather than repeating setup boilerplate or large inline string literals across test cases.
- Prefer real Mise-managed language servers over mock LSP scripts in integration tests when verifying language server lifecycle and protocol behavior.
- Structure all Rust unit and integration tests with nested `mod given_*` and `mod when_*` modules and `fn should_<behavior>()` leaf cases:

```rust
// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // fixtures ----------------------------------------------------------------

    fn sample_agent() -> Identity {
        Identity::new_agent(Uuid::new_v4(), "claude-sonnet-4-5", "anthropic", "claude-sonnet-4-5").unwrap()
    }

    // tests -------------------------------------------------------------------

    mod given_an_agent_identity {
        use super::*;

        mod when_formatting_export_body {
            use super::*;

            #[test]
            fn should_append_agent_disclosure_footer() {
                // Arrange
                let agent = sample_agent();
                // Act
                let body = format_body_with_disclosure("Check invariant", &agent);
                // Assert
                assert!(body.ends_with("> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM"));
            }
        }
    }
}
```

### TypeScript Vitest E2E Tests

- Organize Vitest specs under `e2e/` using `describe('Given ... / When ...')` or `describe('When ...')` blocks and space-separated `it('should <behavior>')` titles (do not use `should_snake_case` in TypeScript).
- Separate `// Arrange`, `// Act`, and `// Assert` blocks with comments. Keep feature setup in adjacent `*.fixtures.ts` files and export only helpers used across modules.
- Keep E2E `test(...)` cases focused on a single behavior rather than chaining unrelated assertions in one test block, and query single-use locators inline rather than exporting one-line `getByTestId` wrapper functions from `fixtures.ts`.
- Separate desktop workspace E2E suites into `Given a new workspace` (creation, persistence, and reopening) and `Given an existing workspace` (viewing, editing, creating, renaming, conflict handling, and deleting entries) with nested `When ...` action blocks, and keep desktop settings E2E specs in a dedicated `e2e/desktop/workspace-settings.spec.ts` suite (`Given no global settings`, `Given global settings and no local settings`, `Given global and local settings`).
- Separate daemon lifecycle tests (`apps/lgtm-cli/tests/daemon.rs`, `e2e/cli/daemon.spec.ts`) into `Given a stopped daemon` and `Given a running daemon` contexts with nested `When ...` blocks for each action (`up`, `status`, `down`, and HTTP/RPC requests), and keep `lgtm open` tests in dedicated `open` suites (`apps/lgtm-cli/tests/open.rs`, `e2e/cli/open.spec.ts`) structured with an outer `When running open` block and inner `Given ...` blocks for each precondition combination.

```typescript
describe(
  'Given a local working tree / When staging and committing in local snapshot mode',
  { tags: ['e2e', 'cli', 'local-review'] },
  () => {
    const fixture = useTestHarness('cli')

    it(
      'should stage and shadow commit worktree changes without modifying checkout',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        await initLocalRepo(runtime)

        // Act
        const added = await runtime.json(['add', 'src/lib.rs', '--local'])

        // Assert
        expect(added).toEqual({ staged_paths: ['src/lib.rs'], local: true })
      },
    )
  },
)
```

- During development, run focused tests (`premise run e2e:smoke` or a single spec/test filter); reserve full E2E runs for final validation or shared fixture changes. See [`e2e/README.md`](e2e/README.md) for GitHub cassette replay and recording commands.
