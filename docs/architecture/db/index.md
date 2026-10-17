# Review database — software architecture

## Overview

`lgtm-db` owns the persistence API: entity repositories, SeaORM `Model` inputs/results, connections, transactions and one shared Rust migration history for SQLite/PostgreSQL. SeaORM `Model` types are public; `Entity`/`ActiveModel` module paths and raw connections remain private. Application models and review workflows belong to the [backend](../backend/index.md).

## Requirement

### Functional

- Persist contexts, identities, sessions, threads and flat comments/replies.
- Enforce keys, relationships, scoped forge IDs and one active session per context.
- Support client-owned transactions across entity repositories.
- Store identity details once and reference them through foreign keys.

### Non-Functional

- Share queries and Rust migrations across SQLite and PostgreSQL.
- Keep Entity/ActiveModel module paths and raw connection access outside the public client API.
- Keep database code in the backend and out of the standalone CLI.

## Design

### Components and Responsibilities

![Shared persistence stack](diagrams/persistence.svg)

- `repositories/` defines `IdentityRepository`, `SessionRepository`, `ReviewContextRepository`, `WorkspaceRepository` and `CommentRepository`. Each struct groups reads and writes for its subject.
- `models.rs` defines public model namespaces that re-export only each table’s SeaORM `Model`. No extra record structs or wrappers are defined.
- `connection.rs` owns connection configuration, migration startup and `Database`/`Transaction` handles.
- Private `entities/` modules define SeaORM rows, columns and relations.
- `migration/` owns one ordered Rust history using SchemaManager and SeaQuery builders.

The client exposes `identities`, `sessions`, `review_contexts`, `workspace` and `comments` repository fields. Comments include thread operations. Callers use `db.sessions.get_active(context_id)` and `db.comments.insert_thread(&thread)`; there is no intermediate queries/mutations property.

### Data and Control Flow

Backend services call repository methods, then map returned SeaORM models to application types. Future review RPC handlers call those services; the CLI currently exposes command scaffolds only. The database library imports no backend code and contains no forge provider or network operations.

![Session lifecycle](diagrams/session-lifecycle.svg)

The backend coordinates draft publication and completion in a transaction. Context-owned comments continue across sessions; completed sessions retain metadata without conversation snapshots.

![Notes flow](diagrams/notes.svg)

Notes write review records without Git or network writes.
![Local review flow](diagrams/local-review.svg)

Local reviews use supplied Git metadata and persist review records.
![Forge observation flow](diagrams/forge.svg)

Supplied observations only; live adapters remain deferred.

### Interfaces and Contracts

![Entity relationships](diagrams/entities.svg)

`Database::begin()` returns a client-owned transaction. The database and transaction expose the same repository fields and methods. Each handle shares its connection across its repositories; a transaction shares one native SeaORM transaction through `Arc`, without a mutex or raw connection accessor. Commit explicitly, roll back explicitly, or drop an uncommitted handle to roll back. Backend review actions each own their transaction. To group repository operations and other fallible work, use the database transaction handle directly.

```rust
let tx = database.begin().await?;
tx.comments.update(&comment).await?;
let updated = tx.comments.get_by_id(context_id, comment_id).await?;
tx.commit().await?;
```

Identities have a typed kind: humans require an email, while agents require a model/provider and have no email. Forge account bindings are separate. Sessions, threads and comments store actor foreign keys. The backend batch-loads identity records when building application results. Reads resolve current identity profiles, including in completed sessions; profile editing has no public operation yet, and different model/provider values require a different identity.

Composite foreign keys enforce matching contexts between sessions, threads and comments. Typed context keys have alternative non-null key columns and unique constraints. A partial index enforces one active session. Checks validate states, anchor shapes and scoped remote-ID pairs. Backend services enforce lifecycle rules such as completed metadata immutability; no backend-specific triggers are needed. Active/completed session metadata and posting state/error combinations have explicit shape checks.

### Returned values and writes

Model modules re-export the actual SeaORM types, such as `lgtm_db::models::comments::Model`. Public fields can be edited in memory. Database changes still require a repository write method using the supplied DB client; no raw connection is exposed. SeaORM traits remain available on these models, so this is an API boundary, not an immutable value type. Internal IDs and foreign keys are Uuid values (native uuid in PostgreSQL, uuid_text in SQLite). Context/session/posting discriminators are typed enums; database constraints enforce variant-dependent fields.

```rust
if let Some(mut comment) = database.comments.get_by_id(context_id, comment_id).await? {
    comment.body = "Updated text".to_owned(); // In memory only.
    database.comments.update(&comment).await?;
}
```

Repository updates select writable columns explicitly. Comment updates preserve author, context/thread ownership, originating session, position and creation time. Thread updates preserve author, ownership and original anchors. Session completion forces the active → completed transition and only writes completion fields; creator, creation time and reviewed commits remain unchanged. Models stay as SeaORM inputs/results; supplying modified immutable fields cannot persist those changes.

Identity profile editing is not exposed. Migrations are private startup machinery; migration tests live beside the private module in src/migration/mod.rs, enabled by cfg(test). No other crate imports private migration source.

## Implementation

### Drivers and concurrency

SeaORM 2.0.4, sea-orm-migration 2.0.4 and SeaQuery 1.0.2 compile on Rust 1.94. SQLx 0.9.0 is a transitive driver dependency, not the application's database API. There are no custom dialect adapters, placeholder rewriting or duplicate SQL migrations.

SQLite uses bundled SQLite, foreign keys enabled and the driver's five-second busy timeout. Each pool has one connection and the default journal mode. Backend mutations first advance the context revision, serializing concurrent starts/replies/completions. PostgreSQL obtains the corresponding row lock through that update.

### Migrations and packaging

The fresh initial migration owns schema definitions independently of entity models. Schema and `seaql_migrations` history run inside an explicit transaction, including SQLite DDL. PostgreSQL uses a nested transaction/savepoint. The framework rejects unknown applied migration names and does not checksum source files.

SQLite startup uses a sibling process lock. PostgreSQL deployments run one migration job before parallel application instances. Desktop upgrade and local/remote sync policy remain [future ADR topics](../../../adr/README.md).

The CLI bundles only the current greeting/health RPC router, generated protocol code and static UI. It has no DB driver or private path dependency.

### Verification

Private unit tests are colocated in their source files; shared fixtures live under src/tests/fixtures.rs. Public database transaction tests live in `lgtm-db/tests/`. Backend integration tests exercise shared SQLite/PostgreSQL behavior through the public database API. They cover constraints, migrations, identity normalization, concurrent operations, application transactions, dummy-forge failure and current-content exports. Compile-fail doctests ensure entities and raw connection access remain private.

### Entity declarations

Private entities use SeaORM 2's dense format. A `#[sea_orm::model]` attribute generates the `Relation` enum and `Related` implementations from inline relationship fields:

```rust
#[sea_orm(belongs_to, from = "thread_id", to = "id")]
pub thread: BelongsTo<super::threads::Entity>,
```

Optional foreign keys use `BelongsTo<Option<Entity>>`. Multiple identity roles remain separate fields (author, resolver, updater), so each relationship retains its own join path. Queries return the actual SeaORM `Model` values through `models::<table>::Model`; no duplicated records or wrapper types sit between these values and backend domain models. The ordered shared migration defines database constraints independently of entity macros.

Inside the database module, generated loaders select a unique target by entity and an identity role by relation variant:

```rust
threads::Entity::load()
    .with(threads::Relation::Author)
    .with(threads::Relation::ResolvedBy)
    .with(threads::Relation::UpdatedBy)
    .with(review_contexts::Entity)
    .one(connection)
    .await?;
```

Colocated entity tests exercise these paths and optional links on real SQLite and PostgreSQL. Public client integration tests compose multi-operation transactions across notes, local and forge conversations, publish drafts, start successor sessions and retain multi-identity attribution.

## References

- [Repositories](../../../libs/lgtm-db/src/repositories/mod.rs), [models](../../../libs/lgtm-db/src/models.rs).
- [Connections](../../../libs/lgtm-db/src/connection.rs), [migrations](../../../libs/lgtm-db/src/migration/mod.rs).
- [Backend review architecture](../backend/index.md), [review tests](../../../apps/lgtm-backend/tests).
- [SeaORM shared migrations](https://www.sea-ql.org/SeaORM/docs/migration/writing-migration/).
