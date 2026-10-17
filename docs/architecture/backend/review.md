# Backend application services — software architecture

## Overview

The backend owns application models, validation, review workflows and Markdown exports. The CLI calls the same backend service in process; this change does not add review RPC methods. Real forge adapters belong in the backend later. There is no provider crate or production provider implementation yet.

## Requirement

### Functional

- Resolve the requester identity supplied to a CLI invocation or RPC handler.
- Manage one active session per context and publish drafts on completion.
- Preserve context conversations and identity links across sessions.
- Let callers group database operations and other fallible work in a transaction.
- Exercise external failures through a dummy forge in integration tests.

### Non-Functional

- Use the database's entity repositories without exposing SeaORM entities.
- Keep application behavior equivalent on SQLite and PostgreSQL.
- Preserve the standalone CLI's bundled backend and private database source.

## Design

### Components and Responsibilities

- `models/` contains one application type per file: actors, context keys, sessions, comments, threads, anchors, conversation results and command inputs.
- `services/auth.rs` provides `AuthService`: requester identity resolution and validation. Future authentication and authorization belong here.
- `services/review.rs` provides `ReviewService`: contexts, sessions, comments, threads, lifecycle rules, listing and exports.
- Each domain model owns its conversions to and from the exposed SeaORM models. `models/identities.rs` shares batch identity loading between models.
- `error.rs` defines application errors.
- Services import `database::Database` directly. `ReviewService::new(client, auth, logger)` accepts a pool-backed client and a tracing span for request logging. Connection setup and shutdown belong to the caller.
- Ordinary methods on `ReviewService` reconcile supplied observations and record posting receipts. There are no providers or network calls in the service.
- `lgtm-db::repositories` owns SQL construction and execution. SeaORM entities stay private inside that library.

### Data and Control Flow

CLI handlers resolve context and pass actor inputs to `AuthService`. RPC handlers can call `auth.requester(&request_context)` to resolve the supplied requester. Review actions use the injected auth service to validate the actor, then enforce review rules and call repository methods. Returned SeaORM models are mapped into application models. Names/emails are resolved from identities; model/provider remain fixed per identity.

Multi-write review actions manage their own atomic database transaction. `ReviewService` is one concrete struct with one implementation block and ordinary methods; it has no storage trait, connection type parameter or transaction variant. For lower-level composition, callers begin a transaction on the database client and call entity repositories using that handle.

### Interfaces and Contracts

```rust
use lgtm_backend::{
    models::{Anchor, CommitContext},
    database::Database,
    services::{AuthService, ReviewService},
};

let client = Database::connect(database_url).await?;
let logger = tracing::info_span!("review_request", request_id = request_id);
let auth = AuthService::new(client.clone(), logger.clone());
let requester = auth.identity(identity_id).await?;
let review = ReviewService::new(client.clone(), auth, logger);

let session = review.start_session(&context, &requester, CommitContext::default()).await?;
review.add_comment(&session.id, &requester, "Check this constraint", Anchor::General).await?;

// The caller owns the shared client's lifecycle.
client.close().await;
```

Models own persistence conversion: `database::models::comments::Model::from(&comment)` writes scalar fields and identity IDs; `Comment::from_db_model(row, &client).await?` resolves identities. `from_db_models` batches identity queries when converting a collection.

The source layout is:

```text
src/
├── models/              # Domain types, validation and record conversions
│   ├── actor.rs
│   ├── comment.rs
│   ├── context.rs
│   ├── session.rs
│   ├── thread.rs
│   └── …
├── services/
│   ├── auth.rs          # Requester identity; future authentication/authorization
│   └── review.rs        # One service: review actions and reconciliation
├── error.rs             # Application errors
└── lib.rs
```

The database client supports composition without entities:

```rust
let tx = database.begin().await?;
tx.comments.update(&comment).await?;
let saved = tx.comments.get(context_id, comment_id).await?;
check_other_work().await?; // An error drops the handle and rolls back DB writes.
tx.commit().await?;
```

Use `tx.rollback().await?` for an explicit rollback. Do not call pool-backed service actions while holding the client's SQLite transaction: those actions use their own connection scope.

## Implementation

Service visibility follows the planned endpoint contract, not just today’s CLI callers. Requester validation and context-by-ID loading are private helpers. Session/comment reads, review lifecycle actions and supplied forge observation/posting receipt operations remain public for planned RPCs. No production forge provider is introduced.

### Requester identity

RPC handlers pass `connectrpc::RequestContext` to `AuthService::requester`. The current contract requires exactly one nonempty `x-lgtm-identity` metadata value containing an existing identity ID. Missing, duplicate, invalid-text and unknown identities return errors. The resolver fetches the current identity record on each call; identities do not leak between requests.

```rust
let requester = auth.requester(&context).await?;
let session = review.start_session(&review_context, &requester, commits).await?;
```

This is supplied identity resolution, not credential verification. Authentication and authorization remain future work in `AuthService`; there are no token or access-policy implementations yet. The production greeting/health RPC contracts remain unchanged. A test endpoint verifies metadata resolution over real gRPC and gRPC-Web transports; production review RPCs remain deferred.

CLI `--identity` and `--as`/Git identity selection use the same auth service. Canonical identity creation is shared through the `Actor` model. Authors imported from a forge are conversation data, not requesters; review reconciliation resolves them through the model within the import transaction.

### Transaction helpers

Review actions begin and commit their own transactions. They call repository write methods, converting domain models directly to SeaORM models at the call site. Three private helpers retain useful behavior: `lock_context` serializes context writes, `require_active_session` rejects completed sessions, and `load_thread` scopes a thread read to its context. `load_context` also supports reading through either the pool or the current transaction. The old `_tx` names and thin write wrappers have been removed.

### Application models versus database models

`models::Comment` is an application model with a resolved author and review semantics. `lgtm_db::models::comments::Model` is the actual SeaORM model containing `author_id` and stored scalar fields. It follows SeaORM's per-table naming convention. Its entity and active model names are not re-exported. Callers can modify its public fields in memory; persisting those changes through the supplied DB client requires a repository write method. SeaORM traits remain available on the model, but the client exposes no raw connection for direct ORM writes. All application types, including supplied remote observations and posting receipts, have individual files under `models/`.

### External operations and rollback

A database transaction can roll back database changes after an external operation fails. It cannot undo an external write that already succeeded. Prefer fetching/validating remote data before opening a write transaction. Future forge posts still need durable sending state, receipts and uncertainty reconciliation across separate transactions; holding a database transaction across a network call does not make both systems atomic.

### Verification and packaging

Backend integration tests cover transaction commit, explicit rollback, dropped transactions, nested savepoints, real database errors and dummy-forge failures on SQLite/PostgreSQL. The dummy lives only in `tests/support/forge.rs`. The CLI bundle copies the backend review modules and database source without adding private package dependencies to the publishable CLI.

## References

- [Auth service](../../../apps/lgtm-backend/src/services/auth.rs), [RPC identity tests](../../../apps/lgtm-backend/tests/auth.rs).
- [Review service](../../../apps/lgtm-backend/src/services/review.rs), [models](../../../apps/lgtm-backend/src/models/mod.rs), [identity resolution](../../../apps/lgtm-backend/src/models/identities.rs).
- [Database API](../db/index.md), [CLI handlers](../cli/reviews.md).
- [Transaction tests](../../../apps/lgtm-backend/tests/review_transactions.rs), [dummy forge tests](../../../apps/lgtm-backend/tests/review_remote_contracts.rs).
