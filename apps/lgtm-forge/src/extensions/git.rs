//! Stateless `git` CLI adapter for working-checkout operations (`add`, `commit`, `push`) and `ForgeBacked` diff/blob reads.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    error::{Error, Result},
    extensions::gitoxide,
    utils::resolve_git_paths,
};

/// Stage worktree paths into the working checkout's native `.git/index` via `git add --`.
pub fn stage_paths(git_dir_or_worktree: &str, paths: &[String]) -> Result<Vec<String>> {
    if paths.is_empty() {
        return Err(Error::Invalid("at least one path is required".to_owned()));
    }
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let expanded = gitoxide::collect_worktree_paths(&worktree_dir, paths)?;

    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .arg("add")
        .arg("--")
        .args(&expanded)
        .output()
        .map_err(|err| Error::Git(format!("failed to invoke git add: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Git(format!("git add failed: {}", stderr.trim())));
    }
    Ok(expanded)
}

/// Create a commit on the working checkout's active branch via standard `git commit` using the user's native Git identity.
pub fn create_commit(git_dir_or_worktree: &str, message: Option<&str>) -> Result<(String, String)> {
    let ctx = gitoxide::get_repo_context(git_dir_or_worktree)?;
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;

    let staged_check = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map_err(|err| Error::Git(format!("failed to check staged index: {err}")))?;
    if staged_check.success() {
        let add_all = Command::new("git")
            .arg("-C")
            .arg(&worktree_dir)
            .args(["add", "-A"])
            .output()
            .map_err(|err| Error::Git(format!("failed to stage worktree files: {err}")))?;
        if !add_all.status.success() {
            let stderr = String::from_utf8_lossy(&add_all.stderr);
            return Err(Error::Git(format!("git add -A failed: {}", stderr.trim())));
        }
    }

    let commit_message = message
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
        .unwrap_or("Update working branch");

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(&worktree_dir)
        .args(["-c", &format!("user.name={}", ctx.user_name)])
        .args(["-c", &format!("user.email={}", ctx.user_email)])
        .args(["commit", "--allow-empty", "--cleanup=verbatim", "-F", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() == Ok("1") {
        let parent_ts = Command::new("git")
            .arg("-C")
            .arg(&worktree_dir)
            .args(["log", "-1", "--format=%ct"])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(1_700_000_000);
        let deterministic_date = format!("@{} +0000", parent_ts + 1);
        cmd.env("GIT_AUTHOR_DATE", &deterministic_date)
            .env("GIT_COMMITTER_DATE", &deterministic_date);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| Error::Git(format!("failed to invoke git commit: {err}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(commit_message.as_bytes())
            .map_err(|err| Error::Git(format!("failed to write commit message: {err}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| Error::Git(format!("failed to wait on git commit: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Git(format!("git commit failed: {}", stderr.trim())));
    }

    let rev_parse = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|err| Error::Git(format!("failed to resolve HEAD after commit: {err}")))?;
    let sha = String::from_utf8_lossy(&rev_parse.stdout).trim().to_owned();
    Ok((sha, ctx.branch))
}

/// Push the active branch from `git_dir` to `origin/<branch>` via standard `git push`.
pub fn push_branch(git_dir_or_worktree: &str, branch: &str) -> Result<(String, String)> {
    let ctx = gitoxide::get_repo_context(git_dir_or_worktree)?;
    let repository_url = ctx.repository_url.ok_or_else(|| {
        Error::Invalid("working repository has no 'origin' remote configured".to_owned())
    })?;
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;

    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .arg("-c")
        .arg("credential.helper=!gh auth git-credential")
        .arg("push")
        .arg("origin")
        .arg(branch)
        .output()
        .map_err(|err| Error::Git(format!("failed to invoke git push: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Git(format!(
            "git push origin {branch} failed: {}",
            stderr.trim()
        )));
    }
    Ok((ctx.head_sha, repository_url))
}

/// Return whether the `origin` remote of `git_dir_or_worktree` points to a local filesystem path.
pub fn is_local_remote(git_dir_or_worktree: &str) -> Result<bool> {
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .map_err(|err| Error::Git(format!("failed to read remote.origin.url: {err}")))?;
    if !output.status.success() {
        return Ok(false);
    }
    let remote_raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let clean = remote_raw.strip_prefix("file://").unwrap_or(&remote_raw);
    Ok(Path::new(clean).exists())
}

/// Update `ref_name` in the working checkout to point at `commit_sha` via `git update-ref`.
pub fn set_reference(git_dir_or_worktree: &str, ref_name: &str, commit_sha: &str) -> Result<()> {
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["update-ref", ref_name, commit_sha.trim()])
        .output()
        .map_err(|err| Error::Git(format!("failed to invoke git update-ref: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Git(format!(
            "git update-ref {ref_name} failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

/// Return all relative file paths modified, added, or removed between `old_sha` and `new_sha` in the working checkout via `git diff --name-only`.
pub fn get_changed_files(
    git_dir_or_worktree: &str,
    old_sha: &str,
    new_sha: &str,
) -> Result<Vec<String>> {
    if old_sha.trim() == new_sha.trim() {
        return Ok(Vec::new());
    }
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["diff", "--name-only", old_sha.trim(), new_sha.trim()])
        .output()
        .map_err(|err| Error::Git(format!("failed to invoke git diff --name-only: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Git(format!(
            "git diff --name-only failed: {}",
            stderr.trim()
        )));
    }
    let mut files: BTreeSet<String> = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            files.insert(trimmed.to_owned());
        }
    }
    Ok(files.into_iter().collect())
}

/// Read the UTF-8 contents of `file_path` at `commit_sha` in the working checkout via `git show <commit_sha>:<file_path>`.
pub fn get_file_at_commit(
    git_dir_or_worktree: &str,
    commit_sha: &str,
    file_path: &str,
) -> Result<Option<String>> {
    let file_path = file_path.trim();
    if file_path.is_empty() {
        return Err(Error::Invalid("file_path must not be empty".to_owned()));
    }
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let spec = format!("{}:{file_path}", commit_sha.trim());
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree_dir)
        .args(["show", &spec])
        .output()
        .map_err(|err| Error::Git(format!("failed to invoke git show {spec}: {err}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Read a map of `file_path -> blob_content` for `paths` at `commit_sha` in the working checkout.
pub fn get_commit_blobs_for_paths(
    git_dir_or_worktree: &str,
    commit_sha: &str,
    paths: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>> {
    let mut blobs = BTreeMap::new();
    for path in paths {
        if let Some(content) = get_file_at_commit(git_dir_or_worktree, commit_sha, path)? {
            blobs.insert(path.clone(), content);
        }
    }
    Ok(blobs)
}
