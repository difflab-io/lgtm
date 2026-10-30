# LGTM

LGTM is being rebuilt around application-owned reviews and explicit publication to an upstream forge; the current application is a hello-review skeleton.

The CLI and Svelte page call the same generated review RPC through a minimal backend and dummy DB package. There is no review persistence, Git snapshot storage, comment lifecycle or forge integration yet. Existing user databases are left untouched. The Neovim package remains a dependency-loading shell.

## Run locally

Install [Mise](https://mise.jdx.dev/getting-started.html), then run from the checkout:

```sh
mise trust
mise install
premise run install
premise run dev
```

Open `http://127.0.0.1:5173` and submit a name to call the backend. To run the bundled application, use `premise run run:cli -- up --no-open`, then `premise run run:cli -- review list --json`. The `ui` command opens the bundled browser interface; `ui --no-open` prints its URL.

The new command shape includes `commit`, `push`, `review list|new|close` and `comment`, with local and agent flags. Only `review list` has a demo RPC response; the other domain operations return `not_implemented` without changing the checkout or upstream forge. See [CLI usage](apps/lgtm-cli/README.md).

## Documentation

- [Development](docs/development.md): builds, generation, checks and packaging.
- [User guide](docs/user-guide.md): available commands and current limits.
- [Architecture](docs/architecture/index.md): the hello-review path and package boundaries.
- [E2E](e2e/README.md): active smoke coverage and retained recording/replay infrastructure.
- [Neovim shell](apps/lgtm.nvim/README.md): loading and pinned dependency installation.
- [Future product direction](prd/0001-core-concepts/README.md), [historical architecture decision](adr/0001-initial-architecture.md) and [historical design prototype](designs/prototypes/lgtm-v0/README.md).

## License

License determination pending.
