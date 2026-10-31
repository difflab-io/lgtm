//! Stateless `gitoxide` (`gix`) and `similar` adapter for local Git repositories, unified diffs, and line-anchor remapping.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use gix::{
    bstr::ByteSlice,
    objs::{
        Commit, Tree,
        tree::{Entry, EntryKind},
    },
    refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
};
use similar::{ChangeTag, TextDiff};

use crate::{
    database::CommentAnchorShift,
    error::{Error, Result},
    models::{Comment, CommentAnchor, DiffHunk, DiffLine, DiffLineKind, Identity},
    utils::{normalize_repository_url, resolve_git_paths},
};

/// Resolved metadata for a local Git working checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoContext {
    pub git_dir: String,
    pub worktree_dir: PathBuf,
    pub branch: String,
    pub head_sha: String,
    pub repository_url: Option<String>,
    pub user_name: String,
    pub user_email: String,
}

/// Metadata and file changes for the current HEAD commit on a local branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadCommitPayload {
    pub head_sha: String,
    pub parent_sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub changed_files: BTreeMap<String, Option<String>>,
}

/// Inspect a local Git repository and return its canonical `git_dir`, worktree path, active branch, `HEAD` SHA, remote URL, and configured author.
pub fn get_repo_context(git_dir_or_worktree: &str) -> Result<RepoContext> {
    let (git_dir_path, worktree_dir, normalized_git_dir) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = match gix::open(&git_dir_path) {
        Ok(repo) => repo,
        Err(_) => gix::open(&worktree_dir).map_err(|err| {
            Error::Git(format!(
                "failed to open repository '{normalized_git_dir}': {err}"
            ))
        })?,
    };

    let branch = repo
        .head_name()
        .map_err(|err| Error::Git(format!("failed to read HEAD name: {err}")))?
        .map(|name| name.shorten().to_str().unwrap_or("HEAD").to_owned())
        .unwrap_or_else(|| "HEAD".to_owned());

    let head_sha = repo
        .head_id()
        .map_err(|err| Error::Git(format!("repository has no unborn/committed HEAD: {err}")))?
        .to_hex()
        .to_string();

    let config = repo.config_snapshot();
    let user_name = config
        .string("user.name")
        .and_then(|v| v.to_str().ok().map(str::trim).map(str::to_owned))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "LGTM Reviewer".to_owned());
    let user_email = config
        .string("user.email")
        .and_then(|v| v.to_str().ok().map(str::trim).map(str::to_owned))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "reviewer@lgtm.local".to_owned());
    let repository_url = config.string("remote.origin.url").and_then(|v| {
        v.to_str()
            .ok()
            .and_then(|raw| normalize_repository_url(raw).ok())
    });

    Ok(RepoContext {
        git_dir: normalized_git_dir,
        worktree_dir,
        branch,
        head_sha,
        repository_url,
        user_name,
        user_email,
    })
}

/// Ensure a bare Git repository exists at `repo_dir`.
pub fn ensure_bare_repo(repo_dir: &Path) -> Result<()> {
    if repo_dir.join("HEAD").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to create snapshot directory '{}': {err}",
            repo_dir.display()
        ))
    })?;
    gix::init_bare(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to initialize bare repository '{}': {err}",
            repo_dir.display()
        ))
    })?;
    Ok(())
}

/// Synchronize objects from `source_git_dir` into `target_repo_dir` so `commit_sha` and its trees/blobs are readable in `target_repo_dir`.
pub fn fetch_objects(source_git_dir: &str, target_repo_dir: &Path, commit_sha: &str) -> Result<()> {
    ensure_bare_repo(target_repo_dir)?;
    let oid = gix::ObjectId::from_hex(commit_sha.trim().as_bytes())
        .map_err(|err| Error::Invalid(format!("invalid commit_sha '{commit_sha}': {err}")))?;
    if let Ok(existing_repo) = gix::open(target_repo_dir)
        && existing_repo.find_object(oid).is_ok()
    {
        return Ok(());
    }

    let (source_git_path, _, _) = resolve_git_paths(source_git_dir)?;
    let source_objects = source_git_path.join("objects");
    let target_objects = target_repo_dir.join("objects");
    copy_objects_dir(&source_objects, &target_objects)?;

    let target_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open target repo: {err}")))?;
    target_repo.find_object(oid).map_err(|err| {
        Error::NotFound(format!(
            "commit '{commit_sha}' was not found after object sync: {err}"
        ))
    })?;
    Ok(())
}

/// Stage worktree paths into the working checkout's staging manifest and `.git/index` (used when `local == false`).
pub fn stage_paths(git_dir_or_worktree: &str, paths: &[String]) -> Result<Vec<String>> {
    let (git_dir_path, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let expanded = collect_worktree_paths(&worktree_dir, paths)?;
    let manifest_path = git_dir_path.join("lgtm-checkout-staged.json");
    let mut staged = read_staged_manifest(&manifest_path)?;

    let repo = gix::open(&git_dir_path)
        .map_err(|err| Error::Git(format!("failed to open checkout repository: {err}")))?;
    stage_worktree_blobs(&repo, &worktree_dir, &expanded, &mut staged)?;
    write_staged_manifest(&manifest_path, &staged)?;
    Ok(expanded)
}

/// Stage worktree paths into a bare snapshot repository without touching the working checkout's `.git/index` (used when `local == true`).
pub fn stage_worktree_to_index(
    git_dir_or_worktree: &str,
    target_repo_dir: &Path,
    branch: &str,
    paths: &[String],
) -> Result<Vec<String>> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    fetch_objects(&ctx.git_dir, target_repo_dir, &ctx.head_sha)?;
    let expanded = collect_worktree_paths(&ctx.worktree_dir, paths)?;

    let snapshot_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open snapshot repo: {err}")))?;
    let manifest_path = snapshot_stage_manifest_path(target_repo_dir, branch);
    let mut staged = read_staged_manifest(&manifest_path)?;

    stage_worktree_blobs(&snapshot_repo, &ctx.worktree_dir, &expanded, &mut staged)?;
    write_staged_manifest(&manifest_path, &staged)?;
    Ok(expanded)
}

/// Create a commit on the working checkout's active branch (used when `local == false`).
pub fn create_commit(
    git_dir_or_worktree: &str,
    message: Option<&str>,
    author: &Identity,
) -> Result<(String, String)> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    let (git_dir_path, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = gix::open(&git_dir_path)
        .map_err(|err| Error::Git(format!("failed to open checkout repository: {err}")))?;

    let manifest_path = git_dir_path.join("lgtm-checkout-staged.json");
    let mut staged = read_staged_manifest(&manifest_path)?;
    if staged.is_empty() {
        let worktree_files = collect_worktree_paths(&worktree_dir, &[".".to_owned()])?;
        stage_worktree_blobs(&repo, &worktree_dir, &worktree_files, &mut staged)?;
    }

    let parent_oid = gix::ObjectId::from_hex(ctx.head_sha.as_bytes())
        .map_err(|err| Error::Git(format!("invalid HEAD oid: {err}")))?;
    let mut tree_entries = read_commit_flat_tree(&repo, parent_oid)?;
    apply_staged_entries(&mut tree_entries, &staged)?;
    let new_tree_oid = write_flat_tree(&repo, &tree_entries)?;

    let ref_name = format!("refs/heads/{}", ctx.branch);
    let commit_message = normalize_commit_message(message, "Update working branch");
    let commit_oid = write_commit_object(
        &repo,
        new_tree_oid,
        vec![parent_oid],
        &commit_message,
        author,
    )?;
    update_ref(&repo, &ref_name, commit_oid)?;
    let _ = std::fs::remove_file(&manifest_path);

    Ok((commit_oid.to_hex().to_string(), ctx.branch))
}

/// Create a commit directly on `ref_name` inside `target_repo_dir` without modifying the working checkout (used when `local == true`).
pub fn create_commit_on_ref(
    git_dir_or_worktree: &str,
    target_repo_dir: &Path,
    ref_name: &str,
    branch: &str,
    message: Option<&str>,
    author: &Identity,
) -> Result<String> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    fetch_objects(&ctx.git_dir, target_repo_dir, &ctx.head_sha)?;
    let head_ref_name = format!("refs/lgtm/heads/{branch}");
    let snapshot_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open snapshot repo: {err}")))?;
    let base_oid = gix::ObjectId::from_hex(ctx.head_sha.as_bytes())
        .map_err(|err| Error::Git(format!("invalid base oid: {err}")))?;
    update_ref(&snapshot_repo, &head_ref_name, base_oid)?;

    let parent_oid = resolve_ref_oid(&snapshot_repo, ref_name).unwrap_or(base_oid);
    let manifest_path = snapshot_stage_manifest_path(target_repo_dir, branch);
    let mut staged = read_staged_manifest(&manifest_path)?;
    if staged.is_empty() {
        let worktree_files = collect_worktree_paths(&ctx.worktree_dir, &[".".to_owned()])?;
        stage_worktree_blobs(
            &snapshot_repo,
            &ctx.worktree_dir,
            &worktree_files,
            &mut staged,
        )?;
    }

    let mut tree_entries = read_commit_flat_tree(&snapshot_repo, parent_oid)?;
    apply_staged_entries(&mut tree_entries, &staged)?;
    let new_tree_oid = write_flat_tree(&snapshot_repo, &tree_entries)?;
    let commit_message = normalize_commit_message(message, "LGTM local snapshot");
    let commit_oid = write_commit_object(
        &snapshot_repo,
        new_tree_oid,
        vec![parent_oid],
        &commit_message,
        author,
    )?;
    update_ref(&snapshot_repo, ref_name, commit_oid)?;
    let _ = std::fs::remove_file(&manifest_path);

    Ok(commit_oid.to_hex().to_string())
}

/// Snapshot the working tree into `target_repo_dir` if needed, returning `(base_sha, head_sha)`.
pub fn snapshot_worktree(
    git_dir_or_worktree: &str,
    target_repo_dir: &Path,
) -> Result<(String, String)> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    fetch_objects(&ctx.git_dir, target_repo_dir, &ctx.head_sha)?;
    let snapshot_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open snapshot repo: {err}")))?;

    let base_oid = gix::ObjectId::from_hex(ctx.head_sha.as_bytes())
        .map_err(|err| Error::Git(format!("invalid base oid: {err}")))?;
    let head_ref_name = format!("refs/lgtm/heads/{}", ctx.branch);
    update_ref(&snapshot_repo, &head_ref_name, base_oid)?;

    let snap_ref_name = format!("refs/lgtm/snapshots/{}", ctx.branch);
    let current_snap_oid = resolve_ref_oid(&snapshot_repo, &snap_ref_name);

    let mut worktree_entries = BTreeMap::new();
    let worktree_files = collect_worktree_paths(&ctx.worktree_dir, &[".".to_owned()])?;
    for rel_path in worktree_files {
        let full_path = ctx.worktree_dir.join(&rel_path);
        if full_path.is_file() {
            let bytes = std::fs::read(&full_path).map_err(|err| {
                Error::Git(format!("failed to read '{}': {err}", full_path.display()))
            })?;
            let blob_id = snapshot_repo
                .write_blob(&bytes)
                .map_err(|err| Error::Git(format!("failed to write snapshot blob: {err}")))?
                .detach();
            worktree_entries.insert(rel_path, blob_id);
        }
    }

    if let Some(existing_oid) = current_snap_oid {
        let existing_tree = read_commit_flat_tree(&snapshot_repo, existing_oid)?;
        if existing_tree == worktree_entries {
            return Ok((ctx.head_sha, existing_oid.to_hex().to_string()));
        }
        let new_tree_oid = write_flat_tree(&snapshot_repo, &worktree_entries)?;
        let author = Identity::new_human(uuid::Uuid::nil(), &ctx.user_name, &ctx.user_email, None)?;
        let commit_oid = write_commit_object(
            &snapshot_repo,
            new_tree_oid,
            vec![existing_oid],
            "LGTM worktree snapshot",
            &author,
        )?;
        update_ref(&snapshot_repo, &snap_ref_name, commit_oid)?;
        return Ok((ctx.head_sha, commit_oid.to_hex().to_string()));
    }

    let base_tree = read_commit_flat_tree(&snapshot_repo, base_oid)?;
    let new_tree_oid = write_flat_tree(&snapshot_repo, &worktree_entries)?;
    let author = Identity::new_human(uuid::Uuid::nil(), &ctx.user_name, &ctx.user_email, None)?;
    let parent_oids = if base_tree == worktree_entries {
        if let Some(first_parent) = read_commit_first_parent(&snapshot_repo, base_oid)? {
            let first_parent_hex = first_parent.to_hex().to_string();
            update_ref(&snapshot_repo, &snap_ref_name, base_oid)?;
            return Ok((first_parent_hex, ctx.head_sha));
        }
        vec![base_oid]
    } else {
        vec![base_oid]
    };

    let commit_oid = write_commit_object(
        &snapshot_repo,
        new_tree_oid,
        parent_oids,
        "LGTM worktree snapshot",
        &author,
    )?;
    update_ref(&snapshot_repo, &snap_ref_name, commit_oid)?;
    Ok((ctx.head_sha, commit_oid.to_hex().to_string()))
}

/// Update `ref_name` in `repo_dir` to point at `commit_sha`.
pub fn set_reference(repo_dir: &Path, ref_name: &str, commit_sha: &str) -> Result<()> {
    let repo = gix::open(repo_dir)
        .map_err(|err| Error::Git(format!("failed to open repository: {err}")))?;
    let oid = gix::ObjectId::from_hex(commit_sha.trim().as_bytes())
        .map_err(|err| Error::Invalid(format!("invalid commit_sha '{commit_sha}': {err}")))?;
    update_ref(&repo, ref_name, oid)
}

/// Return whether the `origin` remote of `git_dir_or_worktree` points to a local filesystem path.
pub fn is_local_remote(git_dir_or_worktree: &str) -> Result<bool> {
    let (git_dir_path, _, _) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = gix::open(&git_dir_path)
        .map_err(|err| Error::Git(format!("failed to open checkout repository: {err}")))?;
    let Some(remote_raw) = repo
        .config_snapshot()
        .string("remote.origin.url")
        .and_then(|v| v.to_str().ok().map(str::to_owned))
    else {
        return Ok(false);
    };
    let clean = remote_raw.strip_prefix("file://").unwrap_or(&remote_raw);
    Ok(Path::new(clean).exists())
}

/// Read the current HEAD commit metadata and file changes relative to its first parent.
pub fn read_head_commit_payload(git_dir_or_worktree: &str) -> Result<HeadCommitPayload> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    let (git_dir_path, _, _) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = gix::open(&git_dir_path)
        .map_err(|err| Error::Git(format!("failed to open checkout repository: {err}")))?;
    let head_oid = parse_oid(&ctx.head_sha)?;
    let object = repo
        .find_object(head_oid)
        .map_err(|err| Error::NotFound(format!("commit '{head_oid}' not found: {err}")))?;
    let commit = object
        .try_into_commit()
        .map_err(|err| Error::Git(format!("object '{head_oid}' is not a commit: {err}")))?;
    let decoded = commit
        .decode()
        .map_err(|err| Error::Git(format!("failed to decode HEAD commit: {err}")))?;
    let parent_oid = decoded.parents().next().ok_or_else(|| {
        Error::Invalid("HEAD commit has no parent commit to push against".to_owned())
    })?;
    let parent_sha = parent_oid.to_hex().to_string();
    let message = decoded.message.to_str_lossy().into_owned();
    let author_name = decoded.author.name.to_str_lossy().into_owned();
    let author_email = decoded.author.email.to_str_lossy().into_owned();
    let timestamp = decoded.author.time.seconds;

    let parent_tree = read_commit_flat_tree(&repo, parent_oid)?;
    let head_tree = read_commit_flat_tree(&repo, head_oid)?;
    let all_paths: BTreeSet<String> = parent_tree
        .keys()
        .chain(head_tree.keys())
        .cloned()
        .collect();
    let mut changed_files = BTreeMap::new();
    for path in all_paths {
        if parent_tree.get(&path) != head_tree.get(&path) {
            match head_tree.get(&path) {
                Some(blob_oid) => {
                    let content = read_blob_string(&repo, *blob_oid)?;
                    changed_files.insert(path, Some(content));
                }
                None => {
                    changed_files.insert(path, None);
                }
            }
        }
    }

    Ok(HeadCommitPayload {
        head_sha: ctx.head_sha,
        parent_sha,
        message,
        author_name,
        author_email,
        timestamp,
        changed_files,
    })
}

/// Push the active branch from `git_dir` to `origin/<branch>`.
pub fn push_branch(git_dir_or_worktree: &str, branch: &str) -> Result<(String, String)> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    let repository_url = ctx.repository_url.ok_or_else(|| {
        Error::Invalid("working repository has no 'origin' remote configured".to_owned())
    })?;
    let (git_dir_path, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = gix::open(&git_dir_path)
        .map_err(|err| Error::Git(format!("failed to open checkout repository: {err}")))?;
    let remote_raw = repo
        .config_snapshot()
        .string("remote.origin.url")
        .and_then(|v| v.to_str().ok().map(str::to_owned))
        .unwrap_or_else(|| repository_url.clone());

    let clean_remote = remote_raw.strip_prefix("file://").unwrap_or(&remote_raw);
    let remote_path = Path::new(clean_remote);
    if remote_path.exists() {
        fetch_objects(&ctx.git_dir, remote_path, &ctx.head_sha)?;
        set_reference(remote_path, &format!("refs/heads/{branch}"), &ctx.head_sha)?;
        return Ok((ctx.head_sha, repository_url));
    }

    let output = std::process::Command::new("git")
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

/// Return all relative file paths modified, added, or removed between `old_sha` and `new_sha`.
pub fn get_changed_files(repo_dir: &Path, old_sha: &str, new_sha: &str) -> Result<Vec<String>> {
    if old_sha.trim() == new_sha.trim() {
        return Ok(Vec::new());
    }
    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let old_oid = parse_oid(old_sha)?;
    let new_oid = parse_oid(new_sha)?;
    let old_tree = read_commit_flat_tree(&repo, old_oid)?;
    let new_tree = read_commit_flat_tree(&repo, new_oid)?;

    let all_paths: BTreeSet<String> = old_tree.keys().chain(new_tree.keys()).cloned().collect();
    let mut changed = Vec::new();
    for path in all_paths {
        if old_tree.get(&path) != new_tree.get(&path) {
            changed.push(path);
        }
    }
    Ok(changed)
}

/// Validate that `file_path` and `line` (plus optional `start_line`) exist within the unified diff between `base_sha` and `head_sha`.
pub fn validate_diff_line(
    repo_dir: &Path,
    base_sha: &str,
    head_sha: &str,
    file_path: &str,
    start_line: Option<u32>,
    line: u32,
) -> Result<()> {
    let file_path = file_path.trim();
    if file_path.is_empty() {
        return Err(Error::Invalid("file_path must not be empty".to_owned()));
    }
    if line == 0 {
        return Err(Error::Invalid("line must be >= 1".to_owned()));
    }
    if let Some(start) = start_line
        && (start == 0 || start > line)
    {
        return Err(Error::Invalid(format!(
            "start_line ({start}) must be between 1 and line ({line})"
        )));
    }

    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let base_oid = parse_oid(base_sha)?;
    let head_oid = parse_oid(head_sha)?;
    let base_tree = read_commit_flat_tree(&repo, base_oid)?;
    let head_tree = read_commit_flat_tree(&repo, head_oid)?;

    let head_blob_oid = head_tree.get(file_path).ok_or_else(|| {
        Error::Invalid(format!(
            "file '{file_path}' does not exist at commit '{head_sha}'"
        ))
    })?;
    let head_content = read_blob_string(&repo, *head_blob_oid)?;
    let total_lines = u32::try_from(head_content.lines().count().max(1)).unwrap_or(u32::MAX);
    if line > total_lines {
        return Err(Error::Invalid(format!(
            "line {line} is out of range for '{file_path}' ({total_lines} lines at '{head_sha}')"
        )));
    }

    let _ = base_tree.get(file_path);
    Ok(())
}

/// Extract unified inline diff hunks around each comment anchor between `base_sha` and `head_sha`.
pub fn get_inline_diff_hunks(
    repo_dir: &Path,
    base_sha: &str,
    head_sha: &str,
    anchors: &[CommentAnchor],
) -> Result<Vec<DiffHunk>> {
    if anchors.is_empty() {
        return Ok(Vec::new());
    }
    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let base_oid = parse_oid(base_sha)?;
    let head_oid = parse_oid(head_sha)?;
    let base_tree = read_commit_flat_tree(&repo, base_oid)?;
    let head_tree = read_commit_flat_tree(&repo, head_oid)?;

    let mut hunks = Vec::with_capacity(anchors.len());
    for anchor in anchors {
        let old_text = match base_tree.get(&anchor.file_path) {
            Some(oid) => read_blob_string(&repo, *oid)?,
            None => String::new(),
        };
        let target_oid = parse_oid(&anchor.commit_sha).unwrap_or(head_oid);
        let target_tree =
            read_commit_flat_tree(&repo, target_oid).unwrap_or_else(|_| head_tree.clone());
        let new_text = match target_tree.get(&anchor.file_path) {
            Some(oid) => read_blob_string(&repo, *oid)?,
            None => String::new(),
        };

        let diff = TextDiff::from_lines(&old_text, &new_text);
        let window_start = anchor
            .start_line
            .unwrap_or(anchor.line)
            .saturating_sub(3)
            .max(1);
        let window_end = anchor.line.saturating_add(3);

        let mut lines = Vec::new();
        let mut new_line_cursor: u32 = 1;
        for change in diff.iter_all_changes() {
            let content = change
                .value()
                .trim_end_matches(&['\r', '\n'][..])
                .to_owned();
            match change.tag() {
                ChangeTag::Equal => {
                    if let Some(new_idx) = change.new_index() {
                        let line_num = u32::try_from(new_idx + 1).unwrap_or(1);
                        new_line_cursor = line_num.saturating_add(1);
                        if line_num >= window_start && line_num <= window_end {
                            lines.push(DiffLine {
                                kind: DiffLineKind::Context,
                                line_number: line_num,
                                content,
                            });
                        }
                    }
                }
                ChangeTag::Insert => {
                    if let Some(new_idx) = change.new_index() {
                        let line_num = u32::try_from(new_idx + 1).unwrap_or(1);
                        new_line_cursor = line_num.saturating_add(1);
                        if line_num >= window_start && line_num <= window_end {
                            lines.push(DiffLine {
                                kind: DiffLineKind::Added,
                                line_number: line_num,
                                content,
                            });
                        }
                    }
                }
                ChangeTag::Delete => {
                    if let Some(old_idx) = change.old_index() {
                        let line_num = u32::try_from(old_idx + 1).unwrap_or(1);
                        if new_line_cursor >= window_start && new_line_cursor <= window_end {
                            lines.push(DiffLine {
                                kind: DiffLineKind::Removed,
                                line_number: line_num,
                                content,
                            });
                        }
                    }
                }
            }
        }

        let start_line = lines
            .first()
            .map(|l| l.line_number)
            .unwrap_or(anchor.start_line.unwrap_or(anchor.line));
        hunks.push(DiffHunk {
            file_path: anchor.file_path.clone(),
            commit_sha: anchor.commit_sha.clone(),
            start_line,
            lines,
        });
    }

    Ok(hunks)
}

/// Recompute comment anchors across `old_head_sha..new_head_sha`, shifting surviving lines and marking edited/deleted lines `outdated = true`.
pub fn recompute_anchors(
    repo_dir: &Path,
    old_head_sha: &str,
    new_head_sha: &str,
    comments: &[Comment],
) -> Result<Vec<CommentAnchorShift>> {
    if old_head_sha.trim() == new_head_sha.trim() {
        return Ok(Vec::new());
    }
    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let old_oid = parse_oid(old_head_sha)?;
    let new_oid = parse_oid(new_head_sha)?;
    let old_tree = read_commit_flat_tree(&repo, old_oid)?;
    let new_tree = read_commit_flat_tree(&repo, new_oid)?;

    let mut shifts = Vec::new();
    for comment in comments {
        let Some(anchor) = &comment.anchor else {
            continue;
        };
        if comment.outdated {
            continue;
        }

        let Some(old_blob_oid) = old_tree.get(&anchor.file_path) else {
            continue;
        };
        let Some(new_blob_oid) = new_tree.get(&anchor.file_path) else {
            shifts.push(CommentAnchorShift {
                comment_id: comment.id,
                start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                line: i32::try_from(anchor.line).ok(),
                commit_sha: Some(anchor.commit_sha.clone()),
                outdated: true,
            });
            continue;
        };

        if old_blob_oid == new_blob_oid {
            if anchor.commit_sha != new_head_sha {
                shifts.push(CommentAnchorShift {
                    comment_id: comment.id,
                    start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                    line: i32::try_from(anchor.line).ok(),
                    commit_sha: Some(new_head_sha.to_owned()),
                    outdated: false,
                });
            }
            continue;
        }

        let old_text = read_blob_string(&repo, *old_blob_oid)?;
        let new_text = read_blob_string(&repo, *new_blob_oid)?;
        let line_map = compute_surviving_line_map(&old_text, &new_text);

        let mapped_line = line_map.get(&anchor.line).copied().flatten();
        let mapped_start = match anchor.start_line {
            Some(start) => line_map.get(&start).copied().flatten().map(Some),
            None => Some(None),
        };

        match (mapped_start, mapped_line) {
            (Some(new_start), Some(new_line)) => {
                shifts.push(CommentAnchorShift {
                    comment_id: comment.id,
                    start_line: new_start.and_then(|v| i32::try_from(v).ok()),
                    line: i32::try_from(new_line).ok(),
                    commit_sha: Some(new_head_sha.to_owned()),
                    outdated: false,
                });
            }
            _ => {
                shifts.push(CommentAnchorShift {
                    comment_id: comment.id,
                    start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                    line: i32::try_from(anchor.line).ok(),
                    commit_sha: Some(anchor.commit_sha.clone()),
                    outdated: true,
                });
            }
        }
    }

    Ok(shifts)
}

fn compute_surviving_line_map(old_text: &str, new_text: &str) -> BTreeMap<u32, Option<u32>> {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut map = BTreeMap::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if let (Some(old_idx), Some(new_idx)) = (change.old_index(), change.new_index()) {
                    let old_line = u32::try_from(old_idx + 1).unwrap_or(1);
                    let new_line = u32::try_from(new_idx + 1).unwrap_or(1);
                    map.insert(old_line, Some(new_line));
                }
            }
            ChangeTag::Delete => {
                if let Some(old_idx) = change.old_index() {
                    let old_line = u32::try_from(old_idx + 1).unwrap_or(1);
                    map.insert(old_line, None);
                }
            }
            ChangeTag::Insert => {}
        }
    }
    map
}

fn parse_oid(sha: &str) -> Result<gix::ObjectId> {
    gix::ObjectId::from_hex(sha.trim().as_bytes())
        .map_err(|err| Error::Invalid(format!("invalid commit SHA '{sha}': {err}")))
}

fn resolve_ref_oid(repo: &gix::Repository, ref_name: &str) -> Option<gix::ObjectId> {
    repo.find_reference(ref_name)
        .ok()
        .and_then(|mut reference| reference.peel_to_id_in_place().ok().map(|id| id.detach()))
}

fn update_ref(repo: &gix::Repository, ref_name: &str, target: gix::ObjectId) -> Result<()> {
    repo.edit_reference(RefEdit {
        change: Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: "lgtm-forge update".into(),
            },
            expected: PreviousValue::Any,
            new: gix::refs::Target::Object(target),
        },
        name: ref_name
            .try_into()
            .map_err(|err| Error::Git(format!("invalid ref name '{ref_name}': {err}")))?,
        deref: false,
    })
    .map_err(|err| Error::Git(format!("failed to update ref '{ref_name}': {err}")))?;
    Ok(())
}

fn read_commit_first_parent(
    repo: &gix::Repository,
    commit_oid: gix::ObjectId,
) -> Result<Option<gix::ObjectId>> {
    let object = repo
        .find_object(commit_oid)
        .map_err(|err| Error::NotFound(format!("commit '{commit_oid}' not found: {err}")))?;
    let commit = object
        .try_into_commit()
        .map_err(|err| Error::Git(format!("object '{commit_oid}' is not a commit: {err}")))?;
    Ok(commit.parent_ids().next().map(|id| id.detach()))
}

fn read_commit_flat_tree(
    repo: &gix::Repository,
    commit_oid: gix::ObjectId,
) -> Result<BTreeMap<String, gix::ObjectId>> {
    let object = repo
        .find_object(commit_oid)
        .map_err(|err| Error::NotFound(format!("commit '{commit_oid}' not found: {err}")))?;
    let commit = object
        .try_into_commit()
        .map_err(|err| Error::Git(format!("object '{commit_oid}' is not a commit: {err}")))?;
    let tree_id = commit
        .tree_id()
        .map_err(|err| Error::Git(format!("failed to read commit tree id: {err}")))?
        .detach();
    let mut flat = BTreeMap::new();
    collect_tree_recursive(repo, tree_id, "", &mut flat)?;
    Ok(flat)
}

fn collect_tree_recursive(
    repo: &gix::Repository,
    tree_oid: gix::ObjectId,
    prefix: &str,
    out: &mut BTreeMap<String, gix::ObjectId>,
) -> Result<()> {
    let object = repo
        .find_object(tree_oid)
        .map_err(|err| Error::Git(format!("failed to find tree '{tree_oid}': {err}")))?;
    let tree = object
        .try_into_tree()
        .map_err(|err| Error::Git(format!("object '{tree_oid}' is not a tree: {err}")))?;
    let tree_ref = tree
        .decode()
        .map_err(|err| Error::Git(format!("failed to decode tree '{tree_oid}': {err}")))?;
    for entry in &tree_ref.entries {
        let name = entry
            .filename
            .to_str()
            .map_err(|err| Error::Git(format!("non-utf8 filename in tree: {err}")))?;
        let rel_path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.mode.is_tree() {
            collect_tree_recursive(repo, entry.oid.into(), &rel_path, out)?;
        } else {
            out.insert(rel_path, entry.oid.into());
        }
    }
    Ok(())
}

fn write_flat_tree(
    repo: &gix::Repository,
    entries: &BTreeMap<String, gix::ObjectId>,
) -> Result<gix::ObjectId> {
    write_subtree(repo, entries, "")
}

fn write_subtree(
    repo: &gix::Repository,
    entries: &BTreeMap<String, gix::ObjectId>,
    prefix: &str,
) -> Result<gix::ObjectId> {
    let mut direct_files: BTreeMap<String, gix::ObjectId> = BTreeMap::new();
    let mut subdirs: BTreeSet<String> = BTreeSet::new();

    for (path, oid) in entries {
        let rel = if prefix.is_empty() {
            path.as_str()
        } else if let Some(stripped) = path.strip_prefix(prefix).and_then(|s| s.strip_prefix('/')) {
            stripped
        } else {
            continue;
        };

        if let Some((dir, _)) = rel.split_once('/') {
            subdirs.insert(dir.to_owned());
        } else {
            direct_files.insert(rel.to_owned(), *oid);
        }
    }

    let mut tree_entries: Vec<Entry> = Vec::new();
    for (filename, oid) in direct_files {
        tree_entries.push(Entry {
            mode: EntryKind::Blob.into(),
            filename: filename.into(),
            oid,
        });
    }
    for dir in subdirs {
        let sub_prefix = if prefix.is_empty() {
            dir.clone()
        } else {
            format!("{prefix}/{dir}")
        };
        let sub_oid = write_subtree(repo, entries, &sub_prefix)?;
        tree_entries.push(Entry {
            mode: EntryKind::Tree.into(),
            filename: dir.into(),
            oid: sub_oid,
        });
    }

    tree_entries.sort_by(|a, b| {
        let a_key = if a.mode.is_tree() {
            format!("{}/", a.filename)
        } else {
            a.filename.to_string()
        };
        let b_key = if b.mode.is_tree() {
            format!("{}/", b.filename)
        } else {
            b.filename.to_string()
        };
        a_key.cmp(&b_key)
    });

    let tree = Tree {
        entries: tree_entries,
    };
    repo.write_object(&tree)
        .map(|id| id.detach())
        .map_err(|err| Error::Git(format!("failed to write tree object: {err}")))
}

fn write_commit_object(
    repo: &gix::Repository,
    tree_oid: gix::ObjectId,
    parents: Vec<gix::ObjectId>,
    message: &str,
    author: &Identity,
) -> Result<gix::ObjectId> {
    let timestamp = if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() == Ok("1") {
        parents
            .first()
            .and_then(|parent_oid| {
                repo.find_object(*parent_oid)
                    .ok()?
                    .try_into_commit()
                    .ok()?
                    .decode()
                    .ok()
                    .map(|c| c.author.time.seconds + 1)
            })
            .unwrap_or(1_700_000_000)
    } else {
        chrono::Utc::now().timestamp()
    };
    let sig = gix::actor::Signature {
        name: author.name.clone().into(),
        email: author.git_author_email().into(),
        time: gix::date::Time::new(timestamp, 0),
    };
    let full_message = if let Some(footer) = author.agent_disclosure_footer() {
        format!("{message}\n\n{footer}\n")
    } else {
        format!("{message}\n")
    };
    let commit = Commit {
        tree: tree_oid,
        parents: parents.clone().into(),
        author: sig.clone(),
        committer: sig.clone(),
        encoding: None,
        message: full_message.clone().into(),
        extra_headers: Vec::new(),
    };
    let _ = repo.write_object(&commit);

    let trimmed_commit = Commit {
        tree: tree_oid,
        parents: parents.into(),
        author: sig.clone(),
        committer: sig,
        encoding: None,
        message: full_message.trim_end_matches('\n').into(),
        extra_headers: Vec::new(),
    };
    repo.write_object(&trimmed_commit)
        .map(|id| id.detach())
        .map_err(|err| Error::Git(format!("failed to write commit object: {err}")))
}

fn read_blob_string(repo: &gix::Repository, blob_oid: gix::ObjectId) -> Result<String> {
    let object = repo
        .find_object(blob_oid)
        .map_err(|err| Error::Git(format!("failed to find blob '{blob_oid}': {err}")))?;
    Ok(String::from_utf8_lossy(&object.data).into_owned())
}

fn stage_worktree_blobs(
    repo: &gix::Repository,
    worktree_dir: &Path,
    paths: &[String],
    staged: &mut BTreeMap<String, Option<String>>,
) -> Result<()> {
    for rel_path in paths {
        let full_path = worktree_dir.join(rel_path);
        if full_path.is_file() {
            let bytes = std::fs::read(&full_path).map_err(|err| {
                Error::Git(format!("failed to read '{}': {err}", full_path.display()))
            })?;
            let blob_id = repo
                .write_blob(&bytes)
                .map_err(|err| Error::Git(format!("failed to write blob: {err}")))?
                .detach();
            staged.insert(rel_path.clone(), Some(blob_id.to_hex().to_string()));
        } else {
            staged.insert(rel_path.clone(), None);
        }
    }
    Ok(())
}

fn apply_staged_entries(
    tree_entries: &mut BTreeMap<String, gix::ObjectId>,
    staged: &BTreeMap<String, Option<String>>,
) -> Result<()> {
    for (path, maybe_hex) in staged {
        match maybe_hex {
            Some(hex) => {
                let oid = parse_oid(hex)?;
                tree_entries.insert(path.clone(), oid);
            }
            None => {
                tree_entries.remove(path);
            }
        }
    }
    Ok(())
}

fn collect_worktree_paths(worktree_dir: &Path, raw_paths: &[String]) -> Result<Vec<String>> {
    if raw_paths.is_empty() {
        return Err(Error::Invalid("at least one path is required".to_owned()));
    }
    let mut collected = BTreeSet::new();
    for raw in raw_paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = Path::new(trimmed);
        if candidate.is_absolute()
            || candidate
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::Invalid(format!(
                "path '{trimmed}' must be a relative path inside the worktree"
            )));
        }
        let target = if trimmed == "." {
            worktree_dir.to_path_buf()
        } else {
            worktree_dir.join(candidate)
        };
        if target.is_dir() {
            collect_files_under_dir(worktree_dir, &target, &mut collected)?;
        } else if target.is_file() {
            let rel = target
                .strip_prefix(worktree_dir)
                .map_err(|err| Error::Invalid(format!("invalid worktree path: {err}")))?
                .to_string_lossy()
                .replace('\\', "/");
            collected.insert(rel);
        } else {
            let rel = trimmed.trim_start_matches("./").replace('\\', "/");
            collected.insert(rel);
        }
    }
    if collected.is_empty() {
        return Err(Error::Invalid(
            "no matching worktree paths found".to_owned(),
        ));
    }
    Ok(collected.into_iter().collect())
}

fn collect_files_under_dir(
    worktree_dir: &Path,
    dir: &Path,
    out: &mut BTreeSet<String>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|err| {
        Error::Git(format!(
            "failed to read directory '{}': {err}",
            dir.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| Error::Git(format!("directory entry error: {err}")))?;
        let file_name = entry.file_name();
        if file_name == ".git" || file_name == "target" || file_name == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files_under_dir(worktree_dir, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(worktree_dir)
                .map_err(|err| Error::Git(format!("strip_prefix error: {err}")))?
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
    Ok(())
}

fn copy_objects_dir(src_dir: &Path, dst_dir: &Path) -> Result<()> {
    if !src_dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst_dir).map_err(|err| {
        Error::Git(format!(
            "failed to create objects dir '{}': {err}",
            dst_dir.display()
        ))
    })?;
    for entry in std::fs::read_dir(src_dir)
        .map_err(|err| Error::Git(format!("failed to read objects dir: {err}")))?
    {
        let entry = entry.map_err(|err| Error::Git(format!("dir entry error: {err}")))?;
        let src_path = entry.path();
        let dst_path = dst_dir.join(entry.file_name());
        if src_path.is_dir() {
            copy_objects_dir(&src_path, &dst_path)?;
        } else if src_path.is_file() && !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path).map_err(|err| {
                Error::Git(format!(
                    "failed to copy git object '{}': {err}",
                    src_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn snapshot_stage_manifest_path(target_repo_dir: &Path, branch: &str) -> PathBuf {
    let safe_branch = branch
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    target_repo_dir.join(format!("lgtm-stage-{safe_branch}.json"))
}

fn read_staged_manifest(path: &Path) -> Result<BTreeMap<String, Option<String>>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|err| Error::Git(format!("failed to read stage manifest: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| Error::Git(format!("failed to parse stage manifest: {err}")))
}

fn write_staged_manifest(path: &Path, staged: &BTreeMap<String, Option<String>>) -> Result<()> {
    let serialized = serde_json::to_string(staged)
        .map_err(|err| Error::Git(format!("failed to serialize stage manifest: {err}")))?;
    std::fs::write(path, serialized)
        .map_err(|err| Error::Git(format!("failed to write stage manifest: {err}")))
}

fn normalize_commit_message(message: Option<&str>, fallback: &str) -> String {
    message
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}
