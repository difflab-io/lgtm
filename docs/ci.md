# CI, Release, and Publishing Architecture

## Overview

LGTM runs continuous integration and releases with [Premise](https://github.com/cloudvoyant/premise) (`pm` / `premise`) and GitHub Actions. All Rust crates set `publish = false` in `Cargo.toml`, and all Bun packages set `"private": true` in `package.json`. GitHub Releases distribute precompiled `lgtm` CLI archives and native `lgtm-desktop` installers across Linux, macOS, and Windows.

## Workflows and Triggers

```mermaid
flowchart LR
    PUSH_BR["Branch push or external PR\n(.github/workflows/on-commit.yml)"] --> PLAN["pm ci plan"]
    PUSH_MAIN["Push to main\n(.github/workflows/on-merge.yml)"] --> PLAN
    PLAN --> MATRIX["pm ci flow-matrix\n(Linux, macOS, Windows)"]
    MATRIX --> E2E["e2e suite"]
    MATRIX -->|"HEAD has [publish-rc] or branch is main"| REL["pm release\n(GitHub Releases)"]
    REL --> PKG["pm packages\n(skips private packages)"]
```

| Workflow | File | Trigger | Release Condition |
| --- | --- | --- | --- |
| On Commit | `.github/workflows/on-commit.yml` | Branch `push` or external fork `pull_request` | Publishes `-rc.<run_number>` when the `HEAD` commit message contains `[publish-rc]` |
| On Merge | `.github/workflows/on-merge.yml` | `push` to `main` | Publishes the next stable SemVer tag (`vX.Y.Z`) on every merge |

## Binary Release Pipeline

```mermaid
flowchart LR
    subgraph Workspace["Workspace Crates and Packages (Private)"]
        CLI["apps/lgtm-cli"] --> FORGE["apps/lgtm-forge"]
        CLI --> ENV["libs/lgtm-env"]
        CLI --> CFG["libs/lgtm-config"]
        FORGE --> DB["libs/lgtm-db"]
        DESK["apps/lgtm-desktop"] --> WEB["apps/lgtm-web"]
        DESK --> LSP["libs/lgtm-lsp"]
        DESK --> ENV
        DESK --> CFG
    end

    subgraph Release["GitHub Release Assets"]
        CLI -->|"release:build"| CLI_ART["lgtm-cli-{linux,macos,windows}\nlgtm-<ver>-<os>-<arch>.tar.gz"]
        DESK -->|"release:build"| DESK_ART["lgtm-desktop-{linux,macos,windows}\n.dmg / .deb / .AppImage / .exe"]
        CLI_ART --> GH[("GitHub Release\n+ checksums.txt")]
        DESK_ART --> GH
    end
```

| Project | Release Platforms | Build Task | Staged Artifacts |
| --- | --- | --- | --- |
| `lgtm-cli` | `linux`, `macos`, `windows` | `//apps/lgtm-cli:release:build` | `lgtm-<version>-{linux-x86_64,macos-aarch64,windows-x86_64}.tar.gz` |
| `lgtm-desktop` | `linux`, `macos`, `windows` | `//apps/lgtm-desktop:release:build` | `lgtm-desktop_<version>_{aarch64.dmg,amd64.AppImage,amd64.deb,x64-setup.exe}` |

```mermaid
sequenceDiagram
    participant GHA as GitHub Actions
    participant PM as Premise CLI (pm)
    participant Mise as Mise Tasks
    participant GH as GitHub Releases

    GHA->>PM: pm ci plan
    PM-->>GHA: Target version + platform matrix
    GHA->>PM: pm ci flow-matrix (per project & OS)
    PM->>Mise: install, build, lint, format:check, test
    opt Release active and native platform row
        PM->>Mise: mise run release:build
        Mise-->>GHA: Upload <project>-<platform> artifacts
    end
    GHA->>PM: pm release (publish-release)
    PM->>GH: Tag HEAD and upload archives + checksums.txt via GoReleaser
    GHA->>PM: pm packages (publish-packages)
    Note over PM: Skips crates.io and npm because all packages are private
```

## Local Release Verification

Run the commands below to verify the `lgtm-cli` release archive and GoReleaser configuration locally without publishing:

```sh
tmp_dir="$(mktemp -d)"
RELEASE_VERSION="0.0.0-local" PREMISE_ARTIFACT_DIR="$tmp_dir" mise run //apps/lgtm-cli:release:build
tar -tzvf "$tmp_dir"/lgtm-0.0.0-local-*.tar.gz
rm -rf "$tmp_dir"
mise run release:check
```

## Future Registry Publishing (`CLO-212`)

`crates.io` compiles packages from uploaded source tarballs and rejects crates that depend on unpublished local `path` dependencies. Publishing `lgtm-cli` to `crates.io` uploads internal source code unless workspace crates are published or bundled. [CLO-212](https://linear.app/cloudvoyant/issue/CLO-212/support-and-test-binary-only-multi-crate-and-parent-bundled-workspace) tracks three workspace publishing modes in Premise CLI:

```mermaid
flowchart TD
    MODE["Workspace Publishing Mode (CLO-212)"] --> M1["1. Binary-only release\npublish = false on all crates\nGitHub Releases only (Current)"]
    MODE --> M2["2. Multi-crate registry publish\npublish = ['crates-io'] in dependency order\nlgtm-config/db -> lgtm-env -> lgtm-forge -> lgtm-cli"]
    MODE --> M3["3. Parent-package bundling\npath dependencies in dev/CI\nVendor workspace crates into lgtm-cli at publish time"]
```

| Mode | Configuration | When to Use |
| --- | --- | --- |
| 1. Binary-only release | Set `publish = false` on all crates and ship binaries on GitHub Releases | Current mode while licensing is private |
| 2. Multi-crate registry publish | Set `publish = ["crates-io"]` on SDK crates and `lgtm-cli` and publish in dependency order | When SDK crates are open-sourced |
| 3. Parent-package bundling | Keep `path` dependencies in source and vendor internal crates into `lgtm-cli` during `pm packages` | When publishing only `lgtm-cli` to `crates.io` |

## References

- [DIFF-287: Fix workspace crate linking and package publishing for lgtm-cli](https://linear.app/cloudvoyant/issue/DIFF-287/fix-workspace-crate-linking-and-package-publishing-for-lgtm-cli)
- [CLO-212: Support and test binary-only, multi-crate, and parent-bundled workspace publishing in Premise CLI](https://linear.app/cloudvoyant/issue/CLO-212/support-and-test-binary-only-multi-crate-and-parent-bundled-workspace)
- [Development Guide](development.md)
- [Architecture Overview](architecture/index.md)
