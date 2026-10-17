# Context and identity selection — software architecture

## Overview

`context.rs` resolves a notes path, local branch or PR and selects the actor for a command. It reads Git metadata without changing the repository. Sessions belong to the selected context; there is no global current-session pointer.

## Requirement

### Functional

- Infer context and Git identity from cwd, with explicit overrides.
- Keep branches, PRs and notes separate, even when they share a checkout.
- Record each invocation's human or agent identity without replacing earlier authors.

### Non-Functional

- Read Git files and refs without writing them.
- Reject missing or ambiguous actor information.
- Canonicalize paths and compare directory boundaries.

## Design

### Components and Responsibilities

- `ContextArgs` holds mutually exclusive notes/branch/PR selectors and optional path/repository overrides.
- `GitContextProvider` allows deterministic discovery tests; `LocalGit` invokes read-only Git commands.
- `ActorArgs` selects `--identity`, `--as`, optional `--model`, or Git name/email, then resolves that input through backend `AuthService`.
- `Database` persists normalized context keys and stable identities through SeaORM.

### Data and Control Flow

The command reads cwd, canonicalizes its path, discovers the repository/common Git directory and selects a full branch ref or explicit PR. Outside Git, an unqualified command selects notes. Detached HEAD needs an explicit context. Identity is resolved for mutations, separately from context/session selection. The handler prints context and actor diagnostics on stderr.

### Interfaces and Contracts

```sh
lgtm comments add --as 'ReviewBot <bot@example.com>' --model model-name --body 'Check this constraint'
lgtm comments add --notes ./docs --as 'Alice <alice@example.com>' --body 'Clarify this'
lgtm session show --pr acme/repo#42
```

The model flag requires `--as`. Agent attribution is structured in storage. A future forge payload appends `> Generated via model-name`; this text is not an identity key. Logical identity and the authenticated posting account are separate.

## Implementation

### Context keys and filters

Repositories normalize to lower-case host/owner/repository; remotes without origin use the canonical Git common directory. Branch keys use the full ref, so renames select a different context and a reused ref selects its existing conversation. Linked checkouts map to the same repository. Notes use canonical absolute paths. List filters match descendants on path component boundaries; checkout filters match their containing workspace. `--all` disables inferred filters.

### Actor records

The database uses normalized email/model/provider identity keys. Sessions, threads and comments store identity foreign keys instead of copying names/emails/model/provider. Reads resolve current profile details in batches. Renaming an identity changes its display in active and completed sessions, threads, comments and exports. Model/provider stay fixed for an identity; a different model/provider uses a different identity. Missing or ambiguous identities require an explicit override. A different actor can reply, resolve or complete a session without changing its creator.

## References

- [Context implementation](../../../apps/lgtm-cli/src/context.rs).
- [Review commands](reviews.md) and [database model](../db/index.md).

Remote identities use a forge-host/account key, independent of local email/model identities. A changed remote email does not create a new identity; matching email alone cannot reassign an unknown remote comment to a local agent.
