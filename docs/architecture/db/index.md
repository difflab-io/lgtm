# Dummy DB package

`lgtm-db` is a stateless hello-data package used by the backend review service. Its `Database` facade exposes a named `get_hello` read; it has no SQL connection, schema, migrations, entities, repositories or transaction API.

The package keeps a visible backend storage boundary for future implementation. It does not persist reviews or inspect, migrate or delete existing user database files. SQLite/PostgreSQL configuration and test matrices are removed from the skeleton.

See [backend architecture](../backend/index.md) for the service boundary and [hello-review](../backend/reviews.md) for its only domain RPC.
