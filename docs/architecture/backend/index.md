# Backend

The backend keeps one domain model (`Review`) and one domain service (`ReviewService`), plus RPC, greeting, health and startup infrastructure. It runs standalone or is bundled into the CLI.

The service reads fixed hello data from the dummy DB facade, normalizes the requested name and constructs a review response. The model owns transport mapping; RPC handlers establish request tracing and call the service. Models do not load or mutate storage.

The old identity, notes, sessions, comment, anchor, diff and forge APIs are removed rather than kept as compatibility paths. Startup does not open a database, apply migrations or obtain forge credentials. Existing user database files are unused.

See the [hello-review contract](reviews.md), [dummy DB](../db/index.md) and [application architecture](../index.md).
