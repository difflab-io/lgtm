# Backend — software architecture

## Overview

The Rust backend owns domain validation, requester identity and review workflows over lgtm-db. The production RPC router currently exposes greeting and health only. Review RPCs and forge adapters are not implemented. The CLI manages the daemon and opens the dummy UI; review/session/comment commands are scaffolds and do not access persistence.

## Requirement

### Functional

- Persist notes, local branch reviews and forge-review conversations through services.
- Track human identities (email) and agent identities (model/provider), independently of forge posting accounts.
- Keep one active session per context shared by identities; publish drafts on completion and retain completed metadata.
- Preserve context-owned threads/comments and scoped forge IDs across later sessions.
- Reconcile supplied complete observations and verified posting receipts without performing network calls.

### Non-Functional

- Keep all database interaction in services; models only validate, map supplied values and implement domain behavior.
- Share transaction semantics across SQLite and PostgreSQL.
- Keep review persistence out of the CLI until backend RPCs exist.
- Inherit caller tracing context without retaining request spans.

## Design

### Components and Responsibilities

```mermaid
flowchart TB
    Backend["Backend"]
    subgraph Library["lgtm-db"]
        Client["Database"]
        Tx["Transaction"]
        subgraph Repositories["Repository fields on either handle"]
            direction LR
            Identity["IdentityRepository"]
            Context["ReviewContextRepository"]
            Workspace["WorkspaceRepository"]
            Session["SessionRepository"]
            Comment["CommentRepository"]
        end
        Models["SeaORM models and entities"]
        Migration["Migrator"]
        Schema["SchemaManager + SeaQuery"]
        Client -->|begin| Tx
        Client --> Repositories
        Tx --> Repositories
        Client -->|connect applies migrations| Migration
        Migration --> Schema
        Repositories --> Models
    end
    Backend --> Client
    Backend -->|commit / rollback| Tx
    Models --> Storage[("SQLite or PostgreSQL")]
    Schema --> Storage
```

- lib.rs: greeting RPC and health router; future review handlers will translate requests into service calls.
- models/: Actor/ActorKind, Context/ContextKey, Session/SessionStatus, Comment/PostingStatus, Thread/Anchor and domain inputs.
- services/auth.rs: requester metadata resolution, explicit human/agent lookup/create and requester validation. Authentication and authorization remain future work.
- services/review.rs: contexts, session lifecycle, comments, resolution, exports and forge reconciliation.
- services/actor.rs: batch actor loading into a plain UUID-to-Actor lookup for service mappings.
- utils.rs: shared formatting/path helpers and forge import/export contracts.
- error.rs: application-defined validation, conflict and not-found errors.
- lgtm-db: repositories, typed SeaORM models, transactions and private migrations.

### Data and Control Flow

Future review RPC handlers establish tracing context, resolve the requester and call ReviewService. The service validates actions, opens a transaction, loads identities/workspace paths and converts supplied DB values into domain models. Domain conversion functions do not perform I/O.

```mermaid
flowchart TD
    A["Active session with draft comments"] -->|Complete in one transaction| C["Completed review metadata"]
    A -->|Publish drafts on completion| P["Context conversation keeps stable comment IDs"]
    C --> N["Next start creates another active session"]
    P --> N
    N --> R["Reply to or resolve existing context threads"]
    P --> L["Bodies and resolution remain current; no snapshots"]
```

Contexts own conversations. Completing a session publishes its drafts and stores a Completed variant with actor/time/outcome/retry metadata. Later reviewers reuse threads rather than copying them. Snapshots are deferred.

### Interfaces and Contracts

Context identity uses these serialized variants:

```json
{"kind":"local","location":{"kind":"branch","git_dir":"/work/project/.git","branch":"refs/heads/main"}}
```

```json
{"kind":"forge_backed","repository_url":"https://github.com/acme/project","number":42}
```

A local notes location contains `{"kind":"notes","path":"/work/notes"}` instead of branch fields. The backend validates absolute local paths and full branch refs. Forge repository URLs require HTTP(S), a host and a namespace/project path, with no credentials, query or fragment. This validates the address shape, not the existence of a live forge repository.


    let db = Database::connect(database_url).await?;
    let auth = AuthService::new(db.clone());
    let human = auth.ensure_human("Alice", "alice@example.com").await?;
    let agent = auth.ensure_agent("Reviewer", "model-name", "provider-name").await?;
    let review = ReviewService::new(db.clone(), auth);

Internal IDs are uuid::Uuid. External forge IDs remain opaque strings scoped by host/context/object kind. Human and agent records have mutually exclusive required fields, checked in Rust and SQL. Context keys distinguish Local locations from ForgeBacked reviews. Local locations are notes paths or Git-directory/branch pairs. Forge-backed reviews require a parsed HTTP(S) repository_url and positive PR/MR number. Forge origins are derived from the URL; remote operations reject local contexts.

SessionStatus separates Active from Completed metadata. PostingStatus only carries an error for Uncertain results; confirmation cannot retain a stale error. SQL constraints protect direct repository callers too.

```mermaid
erDiagram
    direction LR
    REPOSITORIES {
        text repository_url PK
    }
    REVIEW_CONTEXTS {
        uuid id PK
        ContextKind kind
        text notes_path "nullable"
        text git_dir "nullable"
        text branch "nullable"
        text repository_url FK "nullable"
        bigint number "nullable"
        bigint revision
        text last_refreshed_at "nullable"
        text refreshed_head "nullable"
        text refreshed_base "nullable"
        text refresh_error "nullable"
        text refresh_status
    }
    WORKSPACE_PATHS {
        uuid context_id PK,FK
        text path PK
    }
    IDENTITIES {
        uuid id PK
        text name
        IdentityKind kind
        text email "nullable"
        text model "nullable"
        text provider "nullable"
        text forge_account
    }
    SESSIONS {
        uuid id PK
        uuid context_id FK
        uuid creator_id FK
        SessionState state
        text base_commit "nullable"
        text head_commit "nullable"
        text created_at
        text completed_at "nullable"
        uuid completed_by_id FK "nullable"
        text outcome "nullable"
        text body "nullable"
        text forge_review_id "nullable"
        text submission_status
        text submission_error "nullable"
        text request_key "nullable"
        text request_hash "nullable"
    }
    THREADS {
        uuid id PK
        uuid context_id FK
        uuid origin_session_id FK "nullable"
        uuid author_id FK
        text anchor_kind
        text file_path "nullable"
        text side "nullable"
        text reviewed_commit "nullable"
        bigint start_line "nullable"
        bigint end_line "nullable"
        text state
        uuid resolved_by_id FK "nullable"
        text resolved_at "nullable"
        uuid updated_by_id FK "nullable"
        text updated_at
        text forge_thread_id "nullable"
        boolean outdated
    }
    COMMENTS {
        uuid id PK
        uuid context_id FK
        uuid thread_id FK
        uuid origin_session_id FK "nullable"
        uuid author_id FK
        text body
        bigint position
        text visibility
        text created_at
        text updated_at
        text forge_comment_id "nullable"
        text forge_object_kind "nullable"
        text forge_review_id "nullable"
        text forge_body "nullable"
        text forge_updated_at "nullable"
        text posting_account_id "nullable"
        PostingState posting_state
        text posting_error "nullable"
        text posting_request_key "nullable"
        text payload_hash "nullable"
        text remote_presence
    }
    REPOSITORIES |o--o{ REVIEW_CONTEXTS : repository_url
    REVIEW_CONTEXTS ||--o{ WORKSPACE_PATHS : context_id
    REVIEW_CONTEXTS ||--o{ SESSIONS : context_id
    REVIEW_CONTEXTS ||--o{ THREADS : context_id
    REVIEW_CONTEXTS ||--o{ COMMENTS : context_id
    THREADS ||--o{ COMMENTS : thread_id
    SESSIONS |o--o{ THREADS : origin_session_id
    SESSIONS |o--o{ COMMENTS : origin_session_id
    IDENTITIES ||--o{ SESSIONS : creator_id
    IDENTITIES |o--o{ SESSIONS : completed_by_id
    IDENTITIES ||--o{ THREADS : author_id
    IDENTITIES |o--o{ THREADS : resolved_by_id
    IDENTITIES |o--o{ THREADS : updated_by_id
    IDENTITIES ||--o{ COMMENTS : author_id
```

Repositories are db.identities, db.review_contexts, db.sessions, db.comments (including threads) and db.workspace. Reads use get_* names. Begin a transaction to compose operations, then commit or drop to roll back. DB rollback cannot undo external writes.

## Implementation

### Request identity and tracing

AuthService requires one x-lgtm-identity metadata value containing an existing UUID. This is a supplied claim, not credential verification. Profiles are loaded per request.

CLI/RPC boundaries instrument futures. Services emit ordinary tracing macros and do not retain spans. Spawned tasks require explicit propagation. Payloads and credentials are omitted from boundary instrumentation.

### Forge boundary

Only supplied observations and receipts are supported. A fresh complete observation is required before each new PR session. Scoped remote IDs preserve known logical authors. Forge account bindings are separate from agent providers. Uncertain posts require reconciliation. Future forge adapters belong in the backend.

### Verification

Service integration tests cover lifecycle, concurrency, identity normalization, immutable attribution/anchors, dummy-provider failures and remote reconciliation. Public DB-client integration tests compose repositories across notes, local and forge contexts, including successor sessions and multiple actors. Private unit tests are colocated; shared fixtures live in test modules.

## References

- [Database architecture](../db/index.md).
- [Services](../../../apps/lgtm-backend/src/services).
- [Domain models](../../../apps/lgtm-backend/src/models).
- [Integration tests](../../../apps/lgtm-backend/tests).
- [CLI scaffold](../cli/index.md).
