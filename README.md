# LGTM

LGTM is a local code and document review tool for people and agents. The planned browser and Neovim interfaces support local reviews, GitHub pull request reviews, and annotations on arbitrary files, including ignored plans.

## Features

The current scaffold provides a Rust CLI, a bundled browser interface, and a local gRPC server. The interface demonstrates a hello-world request. Review workflows, database storage, and the Neovim plugin are planned.

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

## Docs

- [Development](docs/development.md): local setup, live updates, builds, checks, and releases.
- [Architecture](docs/architecture/index.md): project layout, component connections, and distribution.
- [Architecture decisions](adr/README.md): accepted decisions and their rationale.
- [Product requirements](prd/0001-core-concepts/README.md): planned review behavior.
- [Design prototype](designs/prototypes/lgtm-v0/README.md): review interactions and visual examples.

## License

License determination pending.
