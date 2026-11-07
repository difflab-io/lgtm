# Configuration library (`lgtm-config`) — software architecture

## Overview

`lgtm-config` is the pure-Rust configuration, JSON Schema generation, Specta type definition, language discovery, and workspace `.lgtm/` directory layout library for LGTM (`lgtm-cli`, `lgtm-env`, `lgtm-lsp`, and `lgtm-desktop`). It deep-merges global (`~/.lgtm/config.json`) and project (`<workspace>/.lgtm/config.json`) nested settings (`appearance.theme`, `workspace.activeRoot`, `workspace.recentRoots`, `editor.lineWrapping`) over defaults, provides built-in language presets (Rust, TypeScript, JavaScript, Python, Go, Markdown, and `mise`-managed Biome `json`), generates `schemas/config.schema.json` via `schemars`, detects project roots and workspace languages, and registers `.lgtm/` idempotently in `<git-dir>/info/exclude`.

## Requirement

### Functional

- Layered nested configuration: Load defaults (`UserSettings`), deep-merge `~/.lgtm/config.json` (or `$LGTM_HOME/config.json`), and overlay leaf-level overrides from `<workspace>/.lgtm/config.json` via `figment` inside `ConfigService`.
- Schema and TypeScript parity: Derive `schemars::JsonSchema` and `specta::Type` on all nested settings and patch structs so `schemas/config.schema.json` and `apps/lgtm-web/src/lib/gen/bindings.ts` share a single Rust source of truth.
- Language and root discovery: Resolve a file's language from its extension and walk ancestor directories up to the workspace boundary to find the nearest language root marker (`Cargo.toml`, `tsconfig.json`, `package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`, `.marksman.toml`, `biome.json`, `biome.jsonc`).
- Git worktree hygiene: Create `<workspace>/.lgtm/` and register `.lgtm/` idempotently in `<git-dir>/info/exclude` (supporting both standard `.git` directories and worktree `.git` pointer files) without modifying tracked `.gitignore` files.

### Non-Functional

- Deterministic output: Use `BTreeMap` and `BTreeSet` across all configuration and language discovery models so merged configurations and detected language lists are deterministic.
- Pure configuration boundary: Keep `lgtm-config` models focused on serialization, deserialization, schema generation, and preset lookup while `ConfigService` owns cross-scope merging and filesystem I/O; delegate `mise` binary management and `.lgtm/mise.toml` synthesis/execution to [`lgtm-env`](../env/index.md).

## Design

### Usage

```rust
use std::path::Path;
use lgtm_config::ConfigService;

let workspace_root = Path::new("/path/to/workspace");
let config_service = ConfigService::new();

// Merge built-in presets, ~/.lgtm/config.json, and <workspace>/.lgtm/config.json
let config = config_service.get_config(workspace_root)?;

// Load and deep-merge nested user settings across defaults, global config, and project config
let snapshot = config_service.get_settings_snapshot(Some(workspace_root))?;

// Ensure ~/.lgtm/config.json or <workspace>/.lgtm/config.json exists with $schema
let global_path = config_service.ensure_global_config_file()?;
let local_path = config_service.ensure_local_config_file(workspace_root)?;

// Ensure <workspace>/.lgtm/ and <git-dir>/info/exclude are initialized
let lgtm_dir = config_service.ensure_workspace_dir(workspace_root)?;

// Discover enabled languages present in the workspace
let languages = ConfigService::detect_workspace_languages(&config, workspace_root);
```

### Components and Responsibilities

```mermaid
flowchart TB
    ENV["lgtm-env (MiseService)"]
    LSP["lgtm-lsp (LgtmLspManager)"]
    DESKTOP["lgtm-desktop (WorkspaceService / LspService)"]
    subgraph CfgLib["lgtm-config"]
        CFG_SVC["ConfigService"]
        SETTINGS_MOD["UserSettings, UserSettingsPatch, ResolvedConfigSnapshot"]
        CFG_MOD["LgtmConfig, LanguageConfig, LspSettings"]
        ERR["LgtmConfigError"]
        CFG_SVC --> SETTINGS_MOD
        CFG_SVC --> CFG_MOD
        CFG_SVC --> ERR
    end
    ENV --> CFG_SVC
    LSP --> CFG_SVC
    DESKTOP --> CFG_SVC
    CFG_SVC --> FS[("~/.lgtm/config.json\n<workspace>/.lgtm/config.json\n<git-dir>/info/exclude")]
```

- `ConfigService`: Resolves `<lgtm_home>`, global and workspace configuration paths (`config.json`), initializes default config files with `DEFAULT_SCHEMA_URL`, deep-merges `UserSettings` / `UserSettingsPatch` via `figment`, overlays runtime preset overrides onto `LgtmConfig::default_presets()`, discovers ancestor project roots and workspace languages, creates `<workspace>/.lgtm/`, and updates `<git-dir>/info/exclude`.
- `UserSettings`, `UserSettingsPatch`, `AppearanceSettings*`, `WorkspaceSettings*`, `EditorSettings*`, `LineWrap`, and `ResolvedConfigSnapshot`: Pure domain models with `optfield`, `schemars::JsonSchema`, and `specta::Type` derives, JSON Schema generation, and versioned jsDelivr `$schema` references (`DEFAULT_SCHEMA_URL`).
- `LgtmConfig`, `LanguageConfig`, `LspSettings`, and `MiseTaskConfig`: Pure domain models holding runtime `[lsp]` and `[languages.<id>]` presets (including `mise`-managed Biome `json`) and extension-to-language lookup.
- `LgtmConfigError`: Typed error enum distinguishing global config I/O and parse errors from project config I/O and parse errors.

### Data and Control Flow

```mermaid
sequenceDiagram
    participant Caller as lgtm-desktop / lgtm-lsp
    participant CfgSvc as ConfigService
    participant Disk as Workspace & Global Filesystem

    Caller->>CfgSvc: get_settings_snapshot(Some(workspace_root))
    opt ~/.lgtm/config.json exists
        CfgSvc->>Disk: read global UserSettingsPatch
    end
    opt <workspace>/.lgtm/config.json exists
        CfgSvc->>Disk: read project UserSettingsPatch
    end
    CfgSvc->>CfgSvc: resolve_effective_settings(global, project) via figment
    CfgSvc-->>Caller: ResolvedConfigSnapshot { effective, global, project, global_path, project_path }
```

### Interfaces and Contracts

| Type / Method | Signature | Description |
| --- | --- | --- |
| `ConfigService::new` | `fn new() -> Self` | Construct a configuration service reading `~/.lgtm/config.json` (or `$LGTM_HOME/config.json`). |
| `ConfigService::from_home` | `fn from_home(home: PathBuf) -> Self` | Construct a configuration service rooted at `<home>/.lgtm/config.json`. |
| `ConfigService::with_global_config_path` | `fn with_global_config_path(path: PathBuf) -> Self` | Construct a configuration service with an explicit global config path for isolated tests. |
| `ConfigService::get_lgtm_home_dir` | `fn get_lgtm_home_dir(&self) -> PathBuf` | Return the resolved LGTM home directory (`$LGTM_HOME` or `~/.lgtm`). |
| `ConfigService::get_global_config_path` | `fn get_global_config_path(&self) -> &Path` | Return the path to `~/.lgtm/config.json` (or `$LGTM_HOME/config.json`). |
| `ConfigService::get_local_config_path` | `fn get_local_config_path(&self, workspace_root: &Path) -> PathBuf` | Return the path to `<workspace>/.lgtm/config.json`. |
| `ConfigService::ensure_global_config_file` | `fn ensure_global_config_file(&self) -> Result<PathBuf>` | Ensure `~/.lgtm/config.json` exists on disk with `DEFAULT_SCHEMA_URL` and return its canonical path. |
| `ConfigService::ensure_local_config_file` | `fn ensure_local_config_file(&self, workspace_root: &Path) -> Result<PathBuf>` | Ensure `<workspace>/.lgtm/config.json` exists on disk with `DEFAULT_SCHEMA_URL` and return its canonical path. |
| `ConfigService::get_config` | `fn get_config(&self, workspace_root: &Path) -> Result<LgtmConfig>` | Return merged built-in presets, global config, and `<workspace>/.lgtm/config.json`. |
| `ConfigService::get_settings_snapshot` | `fn get_settings_snapshot(&self, workspace_root: Option<&Path>) -> Result<ResolvedConfigSnapshot>` | Return deep-merged `UserSettings` plus global and project `UserSettingsPatch` trees. |
| `ConfigService::ensure_workspace_dir` | `fn ensure_workspace_dir(&self, workspace_root: &Path) -> Result<PathBuf>` | Ensure `<workspace>/.lgtm/` and `<git-dir>/info/exclude` are initialized. |
| `ConfigService::detect_project_root` | `fn detect_project_root(workspace_root: &Path, file_path: &Path, language: &LanguageConfig) -> (PathBuf, Option<String>)` | Walk ancestor directories up to `workspace_root` to find the nearest root marker. |
| `ConfigService::detect_workspace_languages` | `fn detect_workspace_languages(config: &LgtmConfig, workspace_root: &Path) -> Vec<String>` | Scan workspace root and subdirectories for enabled language root markers and extensions. |
| `LgtmConfig::detect_file_language` | `fn detect_file_language(&self, file_path: &Path) -> Option<(String, &LanguageConfig)>` | Resolve the enabled language ID and configuration for a file extension. |

## Implementation

### Package layout

```text
libs/lgtm-config/
├── Cargo.toml
├── mise.toml
├── README.md
├── src/
│   ├── lib.rs                 Public ConfigService, UserSettings*, and LgtmConfig exports
│   ├── error.rs               LgtmConfigError and Result alias
│   ├── models/
│   │   ├── mod.rs
│   │   ├── config.rs          LgtmConfig, LanguageConfig, presets, and detection logic
│   │   └── settings.rs        UserSettings, UserSettingsPatch, JSON Schema, and validation
│   └── services/
│       ├── mod.rs
│       └── config.rs          ConfigService, JSON persistence, and .git/info/exclude hygiene
└── tests/
    └── config.rs              ConfigService BDD integration suite
```

## References

- [Environment library (`lgtm-env`) architecture](../env/index.md).
- [LSP library (`lgtm-lsp`) architecture](../lsp/index.md).
- [Desktop application (`lgtm-desktop`) architecture](../desktop/index.md).
- [Application architecture](../index.md).
