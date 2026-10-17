# CLI reviews — software architecture

## Overview

The CLI exposes argument shapes pending backend review RPCs. It must not connect to review persistence or invoke ReviewService in-process.

## Requirement

### Functional

- Preserve daemon startup/reuse, status/shutdown and opening the dummy UI.
- Keep review/session/comment argument shapes discoverable.
- Report unimplemented backend review RPCs explicitly.

### Non-Functional

- No review persistence, forge calls or fixture import in CLI handlers.
- No runtime files from dummy session/comment commands.
- Preserve encoded path URLs and daemon behavior.

## Design

### Components and Responsibilities

ContextArgs and ActorArgs declare argument shapes only. Review handlers keep the command tree and return scaffold errors for session/comment actions. The UI module opens the dummy interface through the daemon. Backend services will own review operations behind future RPCs.

### Data and Control Flow

Review selects a local path, ensures the daemon is ready and opens /?path=.... It creates no session and supplies no fabricated session ID. Dummy commands stop with an unimplemented message before persistence side effects.

### Interfaces and Contracts

Context, attribution and anchor flags are syntax only. No authentication, domain normalization or review action is implied.

## Implementation

### Packaging and checks

Only the greeting/health RPC router and static UI are bundled. The CLI has no DB driver, DB source bundle or private crate dependency. Tests supply an invalid DB URL to prove dummy commands never connect and assert that runtime directories remain absent.

## References

- [CLI README](../../../apps/lgtm-cli/README.md).
- [Backend architecture](../backend/index.md).
- [Scaffold](../../../apps/lgtm-cli/src/reviews.rs).
