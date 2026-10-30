# Hello-review API

`ReviewService.HelloReview` is a demonstration of the shared backend boundary, not a persisted review lifecycle.

```protobuf
service ReviewService {
  rpc HelloReview(HelloReviewRequest) returns (HelloReviewResponse);
}
message HelloReviewRequest { string name = 1; }
message HelloReviewResponse { string message = 1; }
```

The service trims `name` and uses `World` when it is empty, returning `Hello, {name}!`. The backend review model owns the response mapping. The dummy DB supplies fixed hello data without SQL or user-file access.

The CLI calls this method for `review list` and labels its output as demo data. The Svelte frontend calls it through a frontend service that also owns hardcoded display fixtures. A failed request produces an error rather than a fabricated greeting.

Protobuf and Connect generate Rust and TypeScript bindings from [review.proto](../../../apps/lgtm-backend/proto/lgtm/v1/review.proto). The standalone method path is `/lgtm.v1.ReviewService/HelloReview`; the bundled browser adds `/rpc`. Existing infrastructure greeting and health services remain available.

There are no create, close, comment, diff or identity RPCs in this skeleton. See [CLI usage](../../../apps/lgtm-cli/README.md) for the unimplemented command surface.
