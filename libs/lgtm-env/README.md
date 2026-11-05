# lgtm-env

Pure-Rust Mise binary management, `.lgtm/mise.toml` synthesis, and task/tool execution SDK (`MiseService`, `GeneratedMiseFile`, `MiseTaskEntry`, `MiseTaskSummary`) for LGTM. See [Environment & Mise SDK architecture](../../docs/architecture/env/index.md).

## Tasks

```sh
premise run //libs/lgtm-env:build
premise run //libs/lgtm-env:test
premise run //libs/lgtm-env:lint
premise run //libs/lgtm-env:format:check
```
