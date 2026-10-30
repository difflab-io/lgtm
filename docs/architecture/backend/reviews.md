# Reviews — software architecture

## Overview

ReviewService currently provides only a hello-review demonstration; persisted review, comment and publication lifecycles are NA/WIP.

## Requirement

NA/WIP.

### Functional

- FR1: Return a hello response through the shared generated RPC.
- FR2: Trim names and use `World` for empty input.

### Non-Functional

- NFR1: Keep model conversion outside RPC handlers.
- NFR2: Perform no storage, Git or forge mutations.

## Design

NA/WIP.

### Usage

```sh
curl --fail http://127.0.0.1:8080/lgtm.v1.ReviewService/HelloReview \
  -H "Content-Type: application/json" \
  --data '{"name":"World"}'
```

The bundled browser endpoint adds `/rpc` before the service path.

#### Choose the target

NA/WIP.

#### Local context and preservation policy

NA/WIP.

#### Start or find a review

NA/WIP.

#### Add a comment on displayed content

NA/WIP.

#### Add notes without starting a review

NA/WIP.

#### Read and manage discussions

NA/WIP.

#### Place saved discussions on displayed content

NA/WIP.

#### Refresh without replacing the review

NA/WIP.

#### Complete or submit

NA/WIP.

#### Promote notes or export

NA/WIP.

#### Retry after a failure

NA/WIP.

### Components and Responsibilities

`ReviewApi` handles transport, `ReviewService` reads dummy DB data, and `Review` normalizes the name and maps its message to the response.

### Data and Control Flow

CLI `review list` and the frontend service call HelloReview and render actual success/error results; frontend display fixtures do not fabricate RPC success.

#### Start or resume internally

NA/WIP.

#### Save locally and synchronize a PR draft

NA/WIP.

#### Resolve and delete remote feedback

NA/WIP.

### Interfaces and Contracts

```protobuf
service ReviewService {
  rpc HelloReview(HelloReviewRequest) returns (HelloReviewResponse);
}
message HelloReviewRequest { string name = 1; }
message HelloReviewResponse { string message = 1; }
```

The message is `Hello, {name}!`; the dummy DB supplies `Hello`.

#### Public review boundary

HelloReview is the only domain RPC in [review.proto](../../../apps/lgtm-backend/proto/lgtm/v1/review.proto); greeting and health remain infrastructure services.

#### Storage ownership

NA/WIP.

### Anchor ownership

NA/WIP.

#### Original anchors and source evidence

NA/WIP.

#### Mapping and stale content

NA/WIP.

#### GitHub location observations

NA/WIP.

#### Context changes

NA/WIP.

#### Response contracts

NA/WIP.

#### RPC method design and contracts

NA/WIP.

#### Comparison and lifecycle rules

NA/WIP.

#### PR continuity across commits

NA/WIP.

#### Draft visibility and recovery

NA/WIP.

### Rejected submission recovery

NA/WIP.

## Implementation

NA/WIP.

### Internal persistence

NA/WIP.

### Module ownership and extension points

Keep `Review` in `src/models/review.rs`, `ReviewService` in `src/services/review.rs` and transport handling in `src/rpc/review.rs`.

### Acceptance flows

Integration/E2E checks cover name normalization, real CLI/browser RPC calls and error propagation. Review creation, completion and commenting are NA/WIP.

## References

- [Backend](index.md).
- [Dummy DB](../db/index.md).
- [CLI usage](../../../apps/lgtm-cli/README.md).
