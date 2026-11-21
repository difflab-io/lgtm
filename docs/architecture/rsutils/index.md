# Rust utilities library (`lgtm-rsutils`) - software architecture

## Overview

`lgtm-rsutils` provides shared Git, filesystem, and URL utilities for LGTM. It gives the workspace crates one implementation for local Git commands, repository metadata inspection, path resolution, and atomic file writes.

## Requirement

### Functional

- Resolve any checkout directory or Git metadata path to its main repository root and active worktree root.
- List, create, and remove Git worktrees and read the checked-out branch and commit.
- Generate deterministic repository identifiers, worktree directory slugs, and normalized remote repository URLs.

### Non-Functional

- Run all Git, path, and hash operations locally without network or database access.
- Keep all operations stateless so concurrent callers do not share mutable state.
- Write files atomically so interrupted writes do not leave partial files on disk.

## Design

### Components and Responsibilities

```mermaid
flowchart TB
    FORGE["lgtm-forge"]
    PROJECT["lgtm-project (ProjectService)"]
    SESSION["lgtm-session (SessionService)"]

    subgraph RsUtils["libs/lgtm-rsutils"]
        GIT["git\n(resolve_git_paths, list_worktrees, add_worktree, remove_worktree, normalize_repository_url, parse_github_owner_repo)"]
        PATHS["paths\n(branch_to_path_slug, atomic_write)"]
        GIT --> PATHS
    end

    FORGE --> RsUtils
    PROJECT --> RsUtils
    SESSION --> RsUtils
    GIT --> FS[("Local .git, .git/worktrees/*,\nand working checkouts")]
```

- `git`: Resolves `.git`, `gitdir`, and `commondir` paths, lists, creates, and removes `git worktree` checkouts, and normalizes remote Git repository URLs.
- `paths`: Formats worktree branch slugs and performs atomic temporary-file-and-rename writes.

### Data and Control Flow

```mermaid
sequenceDiagram
    participant Caller as lgtm-project / lgtm-forge
    participant Git as lgtm_rsutils::git
    participant Disk as .git & .git/worktrees/*

    Caller->>Git: resolve_git_paths(checkout_path)
    Git->>Disk: Read .git (directory or gitdir pointer) + commondir
    Disk-->>Git: worktree_git_dir, worktree_dir, common_git_dir
    Git-->>Caller: ResolvedGitPaths { main_root, worktree_root, common_git_dir }
    Caller->>Git: list_worktrees(common_git_dir)
    Git->>Disk: Inspect main & linked worktrees via gix
    Disk-->>Git: Worktree paths, HEAD SHAs, and branch refs
    Git-->>Caller: Vec<GitWorktreeEntry>
```

### Interfaces and Contracts

| Module / Function | Signature | Description |
| --- | --- | --- |
| `git::resolve_git_paths` | `fn resolve_git_paths(path: &Path) -> Result<ResolvedGitPaths>` | Resolve any checkout or `.git` path via `gix` to `main_root`, `worktree_root`, `worktree_git_dir`, and `common_git_dir`. |
| `git::list_worktrees` | `fn list_worktrees(repo_root: &Path) -> Result<Vec<GitWorktreeEntry>>` | Discover main and linked worktrees via `gix` (`worktrees()` + `HEAD` inspection). |
| `git::list_local_branches` | `fn list_local_branches(repo_root: &Path) -> Result<Vec<String>>` | List sorted local `refs/heads/*` branch names via `gix`. |
| `git::checkout_branch` | `fn checkout_branch(repo_root: &Path, branch: &str, create: bool) -> Result<String>` | Check out an existing branch or create `-b <branch>` via `git checkout`. |
| `git::add_worktree` | `fn add_worktree(repo_root: &Path, target_dir: &Path, branch: &str, base_ref: Option<&str>) -> Result<GitWorktreeEntry>` | Create a linked Git worktree at `target_dir` (checking out `branch` if it exists locally or creating `-b <branch>` from `base_ref`). |
| `git::remove_worktree` | `fn remove_worktree(repo_root: &Path, worktree_dir: &Path, force: bool) -> Result<()>` | Remove a linked Git worktree (`git worktree remove [--force]`), classify dirty-checkout errors, and clean up leftover directories when forced. |
| `git::get_current_branch` | `fn get_current_branch(path: &Path) -> Option<String>` | Return the checked-out short branch name for `path`, or `None` when detached or unavailable. |
| `git::normalize_repository_url` | `fn normalize_repository_url(raw: &str) -> Result<String>` | Normalize SSH or HTTPS Git remote URLs to `https://<host>/<owner>/<repo>`. |
| `git::parse_github_owner_repo` | `fn parse_github_owner_repo(repository_url: &str) -> Result<(String, String)>` | Extract `(owner, repo)` from a normalized GitHub repository URL. |
| `paths::branch_to_path_slug` | `fn branch_to_path_slug(branch: &str) -> String` | Convert a branch name into a lowercase hyphenated directory slug matching Herdr (`~/.lgtm/worktrees/<project>/<branch-slug>`). |
| `paths::atomic_write` | `fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()>` | Write bytes to a sibling temporary file and rename it onto `path`. |

## Implementation

### Package Layout

```text
libs/lgtm-rsutils/
├── Cargo.toml
├── mise.toml
├── src/
│   ├── lib.rs                 Public module and type exports
│   ├── git.rs                 Git path resolution, worktree helpers, and repository URL normalization
│   └── paths.rs               Branch slugs and atomic file writes
└── tests/
    └── git.rs                 BDD integration suite for git path, worktree, branch, and URL helpers
```

## References

- [Project library (`lgtm-project`) architecture](../project/index.md).
- [Session library (`lgtm-session`) architecture](../session/index.md).
- [Forge architecture (`lgtm-forge`)](../forge/index.md).
