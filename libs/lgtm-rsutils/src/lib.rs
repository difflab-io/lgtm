//! Domain-agnostic Git (`gix`), repository URL normalization, path slug, and atomic file write extension utilities.
#![deny(missing_docs, unreachable_pub)]

mod git;
mod paths;

pub use git::GitWorktreeEntry;
pub use git::ResolvedGitPaths;
pub use git::add_worktree;
pub use git::checkout_branch;
pub use git::get_current_branch;
pub use git::list_local_branches;
pub use git::list_worktrees;
pub use git::normalize_repository_url;
pub use git::parse_github_owner_repo;
pub use git::remove_worktree;
pub use git::resolve_git_paths;
pub use paths::atomic_write;
pub use paths::branch_to_path_slug;
