# CLI architecture

The CLI owns argument parsing, daemon management, browser launching and hosting the bundled application. Domain commands use the generated backend client through `src/rpc_client.rs`; they do not contain review persistence.

`commit`, `push`, `review list|new|close` and `comment` define the proposed domain command surface. `review list` calls the real hello-review RPC and labels the result as demo data. Other domain commands validate arguments and return `not_implemented` without changing checkout or upstream state. `ui` remains a working browser entry point alongside `up`, `down` and `status`.

Each command group lives under `src/commands/`; shared arguments belong in `src/models.rs`, and transport belongs in `src/rpc_client.rs`. The bundle task copies backend/DB source, generated Protobuf code and built web assets into the standalone publishable CLI package.

See [usage](../../../apps/lgtm-cli/README.md), [server](server.md), [daemon](daemon.md) and [store](store.md).
