# LGTM implementation conventions

## Repo Structure

- Keep backend domain models in `apps/lgtm-backend/src/models/`, services in `src/services/`, and application errors in `src/error.rs`.
- Put general helpers and external import/export contracts in `src/utils.rs`. Domain models represent application entities.
- Keep database entities private; expose actual SeaORM models and the repositories required by backend workflows.

## Code Style

- Name public repository and service reads `get_*`.
- Use one inherent `impl` block per type per file. Put instance methods first, associated functions after them, and required trait implementations after the inherent block.
- Separate functions with a blank line. Keep unit tests last, after a `// tests ---` separator padded to column 80.
- Use `// fixtures ---` and `// tests ---` separators padded to column 80 in test files.
- Call standard/library functions directly. Do not add aliases that only forward one method call.
- Document entity purpose, field scope, state semantics and public contracts. Omit filler and redundant implementation narration.
- Use `uuid::Uuid` for internal IDs and foreign keys. Keep external forge IDs as opaque scoped strings; name repository natural keys `key`.
- Use enums for discriminators and state-dependent data; enforce essential shape rules with SQL constraints.

## Test Organization

- Colocate private unit tests in the source file under `#[cfg(test)] mod tests`. Shared unit fixtures belong under `src/tests/`.
- Use one `tests/integration.rs` entry point per crate, with named modules under `tests/integration/` for each service or repository area. Shared integration fixtures belong under `tests/support/`.
- Name tests `should_<behavior>` and keep fixture helpers separate from test cases with the standard separators.
- Test multi-operation transactions and full review lifecycles against real SQLite and PostgreSQL. Use dummy external services.
- Run checks through project Mise tasks invoked by Premise. Keep their source scripts tracked; generated output stays ignored.

## Backend

- Models validate, map data and implement domain behavior. They must not query or mutate the database; services own persistence orchestration and relationship loading.
- Keep pure entity factory methods with their domain model, not in services.
- Humans require email; agents require model and provider and have no email. Forge account bindings are separate from model providers.
- Expose only operations needed by agreed backend workflows. Update explicit mutable columns.
- Establish tracing spans at CLI/RPC boundaries. Services use tracing macros and do not store request spans.
- Keep CLI review/session/comment commands scaffold-only until backend review RPCs exist. Preserve daemon, greeting/health RPC and dummy UI behavior; do not embed review persistence in the CLI.
