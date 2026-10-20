# LGTM implementation conventions

## Repo Structure

- Keep backend domain models in `apps/lgtm-backend/src/models/`, services in `src/services/`, and application errors in `src/error.rs`.
- Put general helpers and external import/export contracts in `src/utils.rs`. Domain models represent application entities.
- Keep database entities private; expose actual SeaORM models and the repositories required by backend workflows. Shared SeaORM entity types belong in `libs/lgtm-db/src/entities/common.rs`.

## Docs

- Use soft wraps for documentation prose. Keep each paragraph on one source line and let the editor wrap it for display. Do not add manual line breaks between sentences or at a fixed column width. Preserve structural line breaks in lists, tables, code, and Mermaid diagrams.
- Embed architecture diagrams as inline Mermaid fences so MmdToImg can preview the source in Neovim. Keep diagram source in the Markdown page, without separate diagram directories or tracked preview images.

## Code Style

- Name public repository and service reads `get_*`.
- Use one inherent `impl` block per type per file. Put instance methods first, associated functions after them, and required trait implementations after the inherent block.
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
- Name tests `should_<behavior>` and keep fixture helpers separate from test cases with the standard separators.
- Test multi-operation transactions and full review lifecycles against real SQLite and PostgreSQL. Use dummy external services.
- Run checks through project Mise tasks invoked by Premise. Keep their source scripts tracked; generated output stays ignored.

## Backend

- Models validate, map data and implement domain behavior. They must not query or mutate the database; services own persistence orchestration and relationship loading.
- Keep pure entity factory methods with their domain model, not in services.
- Humans require email; agents require model and provider and have no email. Forge account bindings are separate from model providers.
- Expose database queries and mutations only through named repository methods. The DB client is the single source of truth for the persistence rules encapsulated by those operations. Do not expose raw connections, generic query builders or raw SQL escape hatches to callers.
- Expose only operations needed by agreed backend workflows. Update explicit mutable columns.
- Establish tracing spans at CLI/RPC boundaries. Services use tracing macros and do not store request spans.
- Keep CLI review/session/comment commands scaffold-only until backend review RPCs exist. Preserve daemon, greeting/health RPC and dummy UI behavior; do not embed review persistence in the CLI.
