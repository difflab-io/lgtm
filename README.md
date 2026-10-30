# LGTM

LGTM is a local code and document review tool for people and agents. The CLI supports local reviews, GitHub pull request reviews, and annotations on arbitrary files, including ignored plans. The Neovim plugin is a shell awaiting a rewrite. The browser review interface is planned.

## Features

The backend provides persistent notes, local reviews and GitHub review RPCs over SQLite with optional PostgreSQL. The CLI uses that API. [lgtm.nvim](apps/lgtm.nvim/README.md) currently retains only its loading and test machinery. The bundled browser remains a hello-world demonstration.

## Quick start

Install [Mise](https://mise.jdx.dev/getting-started.html), then run from the checkout:

```sh
mise trust
mise install
mise run install
mise run dev
```

Open `http://127.0.0.1:5173`. The development command starts the backend and web app together.

To use the published release candidate, install the CLI and start its bundled application:

```sh
cargo install lgtm-cli --version 0.1.0-rc.7 --locked
lgtm up
```

The executable is named `lgtm`. It opens the interface at `http://127.0.0.1:8080`. Use `lgtm status` to inspect the running instance and `lgtm down` to stop it.

The [Neovim shell setup guide](apps/lgtm.nvim/README.md#setup) describes loading the package for development. Review commands and UI are unavailable until the plugin is rewritten.

## Docs

- [Development](docs/development.md): local setup, live updates, builds, checks, and releases.
- [CLI review usage](apps/lgtm-cli/README.md): daemon startup, notes, ignored-directory reviews, export and GitHub review commands.
- [User guide](docs/user-guide.md): available interfaces and links to CLI usage.
- [Neovim shell](apps/lgtm.nvim/README.md): package loading, setup and editor test machinery.
- [End-to-end tests](e2e/README.md): separate backend, CLI and Neovim suites.
- [Database architecture](docs/architecture/db/index.md): schema, adapters, concurrency and migration contracts.
- [Architecture](docs/architecture/index.md): key concepts, review lifecycles, component connections, and distribution.
- [Architecture decisions](adr/README.md): accepted decisions and their rationale.
- [Product requirements](prd/0001-core-concepts/README.md): planned review behavior.
- [Design prototype](designs/prototypes/lgtm-v0/README.md): review interactions and visual examples.

## License

License determination pending.

Backend review models and transaction usage are described in the [backend architecture](docs/architecture/backend/index.md).
