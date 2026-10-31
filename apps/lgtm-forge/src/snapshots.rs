//! Concrete local snapshot store (`~/.lgtm/snapshots/$repoSlug`) backed by `extensions::gitoxide`.

use std::path::PathBuf;

use crate::{
    database::CommentAnchorShift,
    error::Result,
    extensions::gitoxide,
    models::{Comment, CommentAnchor, DiffHunk, Identity},
    utils::{compute_repo_slug, default_snapshots_root},
};

/// Local shadow repository manager that isolates `--local` staging, commits, unified diffs, and line-anchor shifts under `~/.lgtm/snapshots/$repoSlug`.
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

    /// Return (and initialize if needed) the bare snapshot repository directory for `git_dir`.
    pub fn snapshot_dir(&self, git_dir: &str) -> Result<PathBuf> {
        let slug = compute_repo_slug(git_dir)?;
        let dir = self.root_dir.join(slug);
        gitoxide::ensure_bare_repo(&dir)?;
        Ok(dir)
    }

    /// Stage worktree paths into the snapshot repository for `git_dir` without modifying the working checkout's `.git/index`.
    pub fn stage_paths(&self, git_dir: &str, paths: &[String]) -> Result<Vec<String>> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.snapshot_dir(&ctx.git_dir)?;
        gitoxide::stage_worktree_to_index(&ctx.git_dir, &snap_dir, &ctx.branch, paths)
    }

    /// Create a shadow commit on `refs/lgtm/snapshots/<branch>` inside the snapshot repository, returning `(commit_sha, branch)`.
    pub fn commit(
        &self,
        git_dir: &str,
        message: Option<&str>,
        author: &Identity,
    ) -> Result<(String, String)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.snapshot_dir(&ctx.git_dir)?;
        let ref_name = format!("refs/lgtm/snapshots/{}", ctx.branch);
        let commit_sha = gitoxide::create_commit_on_ref(
            &ctx.git_dir,
            &snap_dir,
            &ref_name,
            &ctx.branch,
            message,
            author,
        )?;
        Ok((commit_sha, ctx.branch))
    }

    /// Ensure the current working tree is snapshotted in `~/.lgtm/snapshots/$repoSlug`, returning `(base_sha, head_sha)`.
    pub fn ensure_snapshot(&self, git_dir: &str) -> Result<(String, String)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.snapshot_dir(&ctx.git_dir)?;
        gitoxide::snapshot_worktree(&ctx.git_dir, &snap_dir)
    }

    /// Mirror commit objects up to `head_sha` from the working checkout into `refs/lgtm/heads/<branch>` in the snapshot repository.
    pub fn mirror_checkout_commits(&self, git_dir: &str, head_sha: &str) -> Result<()> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let snap_dir = self.snapshot_dir(&ctx.git_dir)?;
        gitoxide::fetch_objects(&ctx.git_dir, &snap_dir, head_sha)?;
        let head_ref = format!("refs/lgtm/heads/{}", ctx.branch);
        gitoxide::set_reference(&snap_dir, &head_ref, head_sha)
    }

    /// Extract inline unified diff hunks around each comment anchor from the snapshot repository.
    pub fn get_inline_diff_hunks(
        &self,
        git_dir: &str,
        base_sha: &str,
        head_sha: &str,
        anchors: &[CommentAnchor],
    ) -> Result<Vec<DiffHunk>> {
        let snap_dir = self.snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, base_sha);
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, head_sha);
        gitoxide::get_inline_diff_hunks(&snap_dir, base_sha, head_sha, anchors)
    }

    /// Return the list of relative file paths modified between `old_sha` and `new_sha`.
    pub fn get_changed_files(
        &self,
        git_dir: &str,
        old_sha: &str,
        new_sha: &str,
    ) -> Result<Vec<String>> {
        let snap_dir = self.snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, old_sha);
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, new_sha);
        gitoxide::get_changed_files(&snap_dir, old_sha, new_sha)
    }

    /// Validate that `file_path` and `line` (and optional `start_line`) exist in the diff between `base_sha` and `head_sha`.
    pub fn validate_diff_line(
        &self,
        git_dir: &str,
        base_sha: &str,
        head_sha: &str,
        file_path: &str,
        start_line: Option<u32>,
        line: u32,
    ) -> Result<()> {
        let snap_dir = self.snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, base_sha);
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, head_sha);
        gitoxide::validate_diff_line(&snap_dir, base_sha, head_sha, file_path, start_line, line)
    }

    /// Recompute surviving and outdated line anchors across `old_head_sha..new_head_sha`.
    pub fn recompute_anchors(
        &self,
        git_dir: &str,
        old_head_sha: &str,
        new_head_sha: &str,
        comments: &[Comment],
    ) -> Result<Vec<CommentAnchorShift>> {
        let snap_dir = self.snapshot_dir(git_dir)?;
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, old_head_sha);
        let _ = gitoxide::fetch_objects(git_dir, &snap_dir, new_head_sha);
        gitoxide::recompute_anchors(&snap_dir, old_head_sha, new_head_sha, comments)
    }
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new(default_snapshots_root())
    }
}
