# LGTM implementation conventions

- Keep backend domain models in `apps/lgtm-backend/src/models/`. They may validate, map data and implement domain behavior; they must not query or mutate the database. Services load relationships and perform all persistence orchestration.
- Represent human and agent identities explicitly. Humans require an email; agents require a model and provider and do not have an email. Forge account bindings are separate from model providers.
- Use `uuid::Uuid` for internal IDs and foreign keys. External forge IDs remain opaque strings scoped by host/context/object kind. Repository natural keys are named `key`, not `id`.
- Use enums for discriminators and variants for state-dependent data. Mirror essential shape rules with SQL constraints so invalid combinations cannot be persisted.
- Name every public repository read `get_*`. Export only operations used by agreed backend workflows. Writes must update explicit mutable columns.
- Put private unit tests in the same source file under `#[cfg(test)] mod tests`. Shared unit fixtures belong under `src/tests/`. Put public integration tests in each crate’s `tests/`; do not import another crate’s private source.
- Exercise multi-operation transactions and complete review lifecycles against real SQLite and PostgreSQL. Use dummy external services in tests.
- Keep the CLI review/session/comment commands scaffold-only until backend review RPCs are implemented. The CLI must not embed DB code, open a review store or invoke review services in-process. Preserve daemon, health/greeting RPC and dummy UI behavior.
- Run CI checks through each project’s supported Mise lifecycle tasks invoked by Premise. Do not add parallel GitHub Actions jobs for checks that belong in project test hooks.
- Document the purpose of every DB entity and stored field, including ID scope and state/error semantics. Use ordinary Rustdoc summaries, examples and error contracts for public APIs.
- Establish tracing spans at CLI/RPC boundaries. Services use normal tracing macros and do not store request spans.
