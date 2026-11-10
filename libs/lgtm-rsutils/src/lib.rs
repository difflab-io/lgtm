//! Domain-agnostic Git (`gix`), path slug, atomic file write, and URL normalization extension utilities.
#![deny(missing_docs, unreachable_pub)]

mod git;
mod paths;
mod url;

pub use git::GitWorktreeEntry;
pub use git::ResolvedGitPaths;
pub use git::add_worktree;
pub use git::get_current_branch;
pub use git::list_worktrees;
pub use git::remove_worktree;
pub use git::resolve_git_paths;
pub use paths::atomic_write;
pub use paths::branch_to_path_slug;
pub use url::normalize_repository_url;
pub use url::parse_github_owner_repo;
