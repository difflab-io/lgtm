//! Concrete local snapshot store (`~/.lgtm/snapshots/$repoSlug`) backed by `extensions::gitoxide`.

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    error::Result,
    extensions::gitoxide,
    utils::{compute_repo_slug, default_snapshots_root},
};

/// Local shadow repository manager that isolates `--local` staging, commits, and blob reads under `~/.lgtm/snapshots/$repoSlug`.
#[derive(Clone, Debug)]
pub struct SnapshotStore {
    root_dir: PathBuf,
}

impl SnapshotStore {
    /// Construct a `SnapshotStore` rooted at `root_dir`.
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    // Public ------------------------------------------------------------------

    /// Compute the snapshot repository path for `git_dir` without creating it on disk.
    pub fn snapshot_dir(&self, git_dir: &str) -> Result<PathBuf> {
        let slug = compute_repo_slug(git_dir)?;
        Ok(self.root_dir.join(slug))
    }

    /// Ensure and return the bare snapshot repository directory for `git_dir`.
    pub fn ensure_snapshot_dir(&self, git_dir: &str) -> Result<PathBuf> {
        let dir = self.snapshot_dir(git_dir)?;
        gitoxide::ensure_bare_repo(&dir)?;
        Ok(dir)
    }

    /// Stage worktree paths into the snapshot repository for `git_dir` without modifying the working checkout's `.git/index`.
    pub fn stage_paths(&self, git_dir: &str, paths: &[String]) -> Result<Vec<String>> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.ensure_snapshot_dir(&ctx.git_dir)?;
        gitoxide::stage_worktree_to_shadow_ref(git_dir, &snap_dir, &ctx.branch, paths)
    }

    /// Create a shadow commit on `refs/lgtm/snapshots/<branch>` inside the snapshot repository using the human user's native Git identity, returning `(commit_sha, branch)`.
    pub fn commit(&self, git_dir: &str, message: Option<&str>) -> Result<(String, String)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.ensure_snapshot_dir(&ctx.git_dir)?;
        let ref_name = format!("refs/lgtm/snapshots/{}", ctx.branch);
        let commit_sha =
            gitoxide::create_commit_on_ref(git_dir, &snap_dir, &ref_name, &ctx.branch, message)?;
        Ok((commit_sha, ctx.branch))
    }

    /// Ensure the current working tree is snapshotted in `~/.lgtm/snapshots/$repoSlug`, returning `(base_sha, head_sha)`.
    pub fn ensure_snapshot(&self, git_dir: &str) -> Result<(String, String)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.ensure_snapshot_dir(&ctx.git_dir)?;
        gitoxide::snapshot_worktree(git_dir, &snap_dir)
    }

    /// Return the list of relative file paths modified between `old_sha` and `new_sha` in the snapshot repository.
    pub fn get_changed_files(
        &self,
        git_dir: &str,
        old_sha: &str,
        new_sha: &str,
    ) -> Result<Vec<String>> {
        let snap_dir = self.ensure_snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, old_sha);
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, new_sha);
        gitoxide::get_changed_files(&snap_dir, old_sha, new_sha)
    }

    /// Read all UTF-8 file blobs in `commit_sha`'s tree from the snapshot repository.
    pub fn get_commit_blobs(
        &self,
        git_dir: &str,
        commit_sha: &str,
    ) -> Result<BTreeMap<String, String>> {
        let snap_dir = self.ensure_snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, commit_sha);
        gitoxide::get_commit_blobs(&snap_dir, commit_sha)
    }

    /// Read a single UTF-8 file blob at `commit_sha` from the snapshot repository.
    pub fn get_file_at_commit(
        &self,
        git_dir: &str,
        commit_sha: &str,
        file_path: &str,
    ) -> Result<Option<String>> {
        let snap_dir = self.ensure_snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, commit_sha);
        gitoxide::get_file_at_commit(&snap_dir, commit_sha, file_path)
    }
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new(default_snapshots_root())
    }
}
