# LGTM Implementation Conventions

## Hard Rules

- Run builds, formatters, linters, and tests through project [Mise tasks](mise.toml) invoked by Premise (`premise run format`, `premise run lint`, `premise run test`, `premise run e2e:smoke`). Keep task scripts tracked and generated outputs ignored. Define Mise tasks and tools in `[.]mise.toml` files (such as `.lgtm/mise.toml`), never inside `.lgtm/config.json`.
- Store generated JSON Schemas under [`schemas/`](schemas/) at the repository root and reference them via versioned jsDelivr tag URLs (`https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v<version>/schemas/...`) rather than writing local schema copies into `.lgtm/`.
- During alpha, update branch-introduced contracts directly without compatibility shims or duplicate fields.
- Expose database queries and mutations only through named repository methods in [`libs/lgtm-db`](libs/lgtm-db/src/lib.rs). Keep SeaORM `Entity` and `ActiveModel` types `pub(crate)`; never expose raw connections, query builders, or raw SQL to callers.
- Models validate, map data, and implement pure domain behavior; services own persistence orchestration, cross-scope configuration merging, transactions, and external I/O.
- Give each file and service a single responsibility; extract distinct capabilities (such as caching, configuration, or LSP orchestration) into dedicated model and service modules with their own integration suites, combine host services into a single `AppState` in `apps/lgtm-desktop/src/app.rs`, and wire event handling and cross-service coordination in the application entry point (`app.rs` / `Shell.svelte`) rather than emitting framework events or calling sibling services inside domain services.
- Configure user preferences by editing `~/.lgtm/config.json` and `<workspace>/.lgtm/config.json` directly in the editor and applying resolved settings when config files are saved, rather than exposing programmatic preference-mutation UI buttons or RPCs.
- Keep CLI review and comment commands as thin RPC clients over `lgtm-forge`. Preserve daemon and health behavior; do not embed review persistence in `lgtm-cli`.
- Establish tracing spans at CLI/RPC boundaries (`#[tracing::instrument]`). Services emit tracing events and never store request spans.
- Preserve pinned Neovim dependencies in the plugin setup, source-switch installer, and E2E harness (including NuiComponents/NUI and optional Kak).
- [Proposed] Preserve the Neovim development and release plugin installation tasks when resetting plugin features.
- Never create, recreate, rename, archive, or delete GitHub repositories in tests. Use `LGTM_E2E_GITHUB_REPOSITORY`, verify private visibility before writes, and clean up only run-owned PRs and refs.

## Organization

```text
apps/
├── lgtm-cli/       # Thin RPC CLI client, daemon launcher, and terminal/JSON view models
├── lgtm-desktop/   # Tauri desktop host and Specta IPC command handlers
├── lgtm-forge/     # Connect RPC backend, review orchestration, and Git/GitHub adapters
└── lgtm-web/       # Svelte 5 desktop frontend
libs/
├── lgtm-config/    # Nested .lgtm/config.json settings, JSON Schema, and language discovery
├── lgtm-db/        # Private SeaORM entities and named repositories
├── lgtm-env/       # Managed mise binary lifecycle and .lgtm/mise.toml synthesis
├── lgtm-lsp/       # Language server process manager, cache service, and stdio JSON-RPC client
└── lgtm-ui/        # Portable Helical UI components and services
e2e/                # Vitest E2E suites (common, forge, cli, nvim, desktop)
```

- [Proposed] Name architecture documentation and E2E suite directories for `lgtm-forge` as `forge` (`docs/architecture/forge/` and `e2e/forge/`) rather than `backend`.
- [Proposed] Preserve existing documentation template headings and structure; use `NA/WIP` for unimplemented sections.

## Style

### Rust Layout & Naming

- Document all public types, functions, and methods (`pub fn`) with `///` docstrings (`#![warn(missing_docs, unreachable_pub)]`) that state the type's architectural role and boundary purpose.
- Export only public API types from crate roots (`lib.rs`), formatted one `pub use` item per line.
- Link workspace library crates through `Cargo.toml` dependencies rather than `#[path = ...]` module shims or `#[allow(dead_code)]`, and prefer standard library or crate formatting traits (such as `LowerHex` on digest outputs) over manual formatting loops.
- Use one inherent `impl` block per type per file: constructors first, `// Public` operations next, `// Private` helpers last, and trait `impl` blocks after the inherent block. Pad section headers with hyphens to column 80.
- Name public repository and service reads `get_*`. Name mutating operations and factories with imperative verbs (`promote`, `publish_session`).
- Call library and generated client methods directly; do not add pass-through wrapper aliases.
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

### Documentation

- Document all environment variables that affect LGTM behavior in [`docs/environment.md`](docs/environment.md).
- Use soft wraps for Markdown prose (one source line per paragraph). Keep diagrams as inline Mermaid fences and prefer file trees, tables, and short code snippets over long prose.

## Testing

### Rust BDD Unit & Integration Tests

- Colocate unit tests in `#[cfg(test)] mod tests` after a `// tests ---` separator padded to column 80; place shared unit fixtures in `src/tests/`.
- Put integration suites directly under `tests/<area>.rs` with shared fixtures in `tests/common/mod.rs` (never `tests/integration/` or `tests/support/`). Cover core module behaviors, edge cases, and failure modes at library integration boundaries to keep E2E suites focused on cross-tier wiring.
- Do not write custom JSON schema validation/parsing logic or unit tests for schema validation and serialization; rely on library crates and test domain behavior through public service boundaries.
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
- Separate desktop workspace E2E suites into `Given a new workspace` (creation, persistence, and reopening) and `Given an existing workspace` (viewing, editing, creating, renaming, conflict handling, and deleting entries) with nested `When ...` action blocks.

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
