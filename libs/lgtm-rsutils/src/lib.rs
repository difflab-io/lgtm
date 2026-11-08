//! Stateless Git path resolution, worktree operations, path slug, atomic write, and URL utilities for LGTM.
#![deny(missing_docs, unreachable_pub)]

mod error;
mod git;
mod paths;
mod url;

pub use error::Result;
pub use error::RsUtilsError;
pub use git::GitWorktreeEntry;
pub use git::ResolvedGitPaths;
pub use git::add_worktree;
pub use git::get_current_branch;
pub use git::list_worktrees;
pub use git::remove_worktree;
pub use git::resolve_git_paths;
pub use paths::atomic_write;
pub use paths::branch_to_path_slug;
pub use paths::compute_repo_slug;
pub use paths::default_lgtm_home;
pub use url::normalize_repository_url;
pub use url::parse_github_owner_repo;
