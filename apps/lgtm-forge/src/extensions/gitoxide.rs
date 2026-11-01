//! Stateless `gitoxide` (`gix`) adapter for local shadow repositories (`~/.lgtm/snapshots/$repoSlug`) and repository inspection.

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

use crate::{
    error::{Error, Result},
    utils::{normalize_repository_url, resolve_git_paths},
};

/// Resolved metadata for a local Git working checkout or linked worktree.
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

/// Inspect a local Git repository or linked worktree and return its canonical shared `git_dir`, active `worktree_dir`, branch, `HEAD` SHA, remote URL, and configured human author.
pub fn get_repo_context(git_dir_or_worktree: &str) -> Result<RepoContext> {
    let (worktree_git_dir, worktree_dir, normalized_git_dir) =
        resolve_git_paths(git_dir_or_worktree)?;
    let repo = match gix::open(&worktree_dir) {
        Ok(repo) => repo,
        Err(_) => gix::open(&worktree_git_dir).map_err(|err| {
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
    let oid = parse_oid(commit_sha)?;
    if let Ok(existing_repo) = gix::open(target_repo_dir)
        && existing_repo.find_object(oid).is_ok()
    {
        return Ok(());
    }

    let (_, _, common_git_dir) = resolve_git_paths(source_git_dir)?;
    let source_objects = Path::new(&common_git_dir).join("objects");
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

/// Stage worktree paths into `refs/lgtm/staged/<branch>` inside `target_repo_dir` via `gitoxide` without touching the working checkout's `.git/index`.
pub fn stage_worktree_to_shadow_ref(
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
    let base_oid = parse_oid(&ctx.head_sha)?;
    let head_ref_name = format!("refs/lgtm/heads/{branch}");
    update_ref(&snapshot_repo, &head_ref_name, base_oid)?;

    let staged_ref_name = format!("refs/lgtm/staged/{branch}");
    let snap_ref_name = format!("refs/lgtm/snapshots/{branch}");

    let mut tree_entries =
        if let Some(staged_tree_oid) = resolve_ref_oid(&snapshot_repo, &staged_ref_name) {
            let mut flat = BTreeMap::new();
            collect_tree_recursive(&snapshot_repo, staged_tree_oid, "", &mut flat)?;
            flat
        } else if let Some(snap_commit_oid) = resolve_ref_oid(&snapshot_repo, &snap_ref_name) {
            read_commit_flat_tree(&snapshot_repo, snap_commit_oid)?
        } else {
            read_commit_flat_tree(&snapshot_repo, base_oid)?
        };

    for rel_path in &expanded {
        let full_path = ctx.worktree_dir.join(rel_path);
        if full_path.is_file() {
            let bytes = std::fs::read(&full_path).map_err(|err| {
                Error::Git(format!("failed to read '{}': {err}", full_path.display()))
            })?;
            let blob_id = snapshot_repo
                .write_blob(&bytes)
                .map_err(|err| Error::Git(format!("failed to write blob: {err}")))?
                .detach();
            tree_entries.insert(rel_path.clone(), blob_id);
        } else {
            tree_entries.remove(rel_path);
        }
    }

    let staged_tree_oid = write_flat_tree(&snapshot_repo, &tree_entries)?;
    update_ref(&snapshot_repo, &staged_ref_name, staged_tree_oid)?;
    Ok(expanded)
}

/// Create a shadow commit on `ref_name` (`refs/lgtm/snapshots/<branch>`) inside `target_repo_dir` using the human author from `git config`.
pub fn create_commit_on_ref(
    git_dir_or_worktree: &str,
    target_repo_dir: &Path,
    ref_name: &str,
    branch: &str,
    message: Option<&str>,
) -> Result<String> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    fetch_objects(&ctx.git_dir, target_repo_dir, &ctx.head_sha)?;
    let head_ref_name = format!("refs/lgtm/heads/{branch}");
    let staged_ref_name = format!("refs/lgtm/staged/{branch}");
    let snapshot_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open snapshot repo: {err}")))?;
    let base_oid = parse_oid(&ctx.head_sha)?;
    update_ref(&snapshot_repo, &head_ref_name, base_oid)?;

    let parent_oid = resolve_ref_oid(&snapshot_repo, ref_name).unwrap_or(base_oid);
    let new_tree_oid =
        if let Some(staged_tree_oid) = resolve_ref_oid(&snapshot_repo, &staged_ref_name) {
            let _ = delete_ref(&snapshot_repo, &staged_ref_name);
            staged_tree_oid
        } else {
            let worktree_entries = hash_worktree_entries(&snapshot_repo, &ctx.worktree_dir)?;
            write_flat_tree(&snapshot_repo, &worktree_entries)?
        };

    let commit_message = normalize_commit_message(message, "LGTM local snapshot");
    let commit_oid = write_commit_object(
        &snapshot_repo,
        new_tree_oid,
        vec![parent_oid],
        &commit_message,
        &ctx.user_name,
        &ctx.user_email,
    )?;
    update_ref(&snapshot_repo, ref_name, commit_oid)?;

    Ok(commit_oid.to_hex().to_string())
}

/// Snapshot the working tree into `target_repo_dir` idempotently via `gitoxide`, returning `(base_sha, head_sha)`.
pub fn snapshot_worktree(
    git_dir_or_worktree: &str,
    target_repo_dir: &Path,
) -> Result<(String, String)> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    fetch_objects(&ctx.git_dir, target_repo_dir, &ctx.head_sha)?;
    let snapshot_repo = gix::open(target_repo_dir)
        .map_err(|err| Error::Git(format!("failed to open snapshot repo: {err}")))?;

    let base_oid = parse_oid(&ctx.head_sha)?;
    let head_ref_name = format!("refs/lgtm/heads/{}", ctx.branch);
    update_ref(&snapshot_repo, &head_ref_name, base_oid)?;

    let snap_ref_name = format!("refs/lgtm/snapshots/{}", ctx.branch);
    let current_snap_oid = resolve_ref_oid(&snapshot_repo, &snap_ref_name);
    let worktree_entries = hash_worktree_entries(&snapshot_repo, &ctx.worktree_dir)?;

    if let Some(existing_oid) = current_snap_oid {
        let existing_tree = read_commit_flat_tree(&snapshot_repo, existing_oid)?;
        if existing_tree == worktree_entries {
            return Ok((ctx.head_sha, existing_oid.to_hex().to_string()));
        }
        let new_tree_oid = write_flat_tree(&snapshot_repo, &worktree_entries)?;
        let commit_oid = write_commit_object(
            &snapshot_repo,
            new_tree_oid,
            vec![existing_oid],
            "LGTM worktree snapshot",
            &ctx.user_name,
            &ctx.user_email,
        )?;
        update_ref(&snapshot_repo, &snap_ref_name, commit_oid)?;
        return Ok((ctx.head_sha, commit_oid.to_hex().to_string()));
    }

    let base_tree = read_commit_flat_tree(&snapshot_repo, base_oid)?;
    let new_tree_oid = write_flat_tree(&snapshot_repo, &worktree_entries)?;
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
        &ctx.user_name,
        &ctx.user_email,
    )?;
    update_ref(&snapshot_repo, &snap_ref_name, commit_oid)?;
    Ok((ctx.head_sha, commit_oid.to_hex().to_string()))
}

/// Return all relative file paths modified, added, or removed between `old_sha` and `new_sha` inside `repo_dir`.
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

/// Read all UTF-8 file blobs in `commit_sha`'s tree inside `repo_dir`.
pub fn get_commit_blobs(repo_dir: &Path, commit_sha: &str) -> Result<BTreeMap<String, String>> {
    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let commit_oid = parse_oid(commit_sha)?;
    let flat_tree = read_commit_flat_tree(&repo, commit_oid)?;
    let mut blobs = BTreeMap::new();
    for (path, oid) in flat_tree {
        blobs.insert(path, read_blob_string(&repo, oid)?);
    }
    Ok(blobs)
}

/// Read a single UTF-8 file blob at `commit_sha` inside `repo_dir`.
pub fn get_file_at_commit(
    repo_dir: &Path,
    commit_sha: &str,
    file_path: &str,
) -> Result<Option<String>> {
    let file_path = file_path.trim();
    if file_path.is_empty() {
        return Err(Error::Invalid("file_path must not be empty".to_owned()));
    }
    let repo = gix::open(repo_dir).map_err(|err| {
        Error::Git(format!(
            "failed to open repo '{}': {err}",
            repo_dir.display()
        ))
    })?;
    let commit_oid = parse_oid(commit_sha)?;
    let flat_tree = read_commit_flat_tree(&repo, commit_oid)?;
    let Some(blob_oid) = flat_tree.get(file_path) else {
        return Ok(None);
    };
    Ok(Some(read_blob_string(&repo, *blob_oid)?))
}

/// Read the current HEAD commit metadata and file changes relative to its first parent.
pub fn get_head_commit_payload(git_dir_or_worktree: &str) -> Result<HeadCommitPayload> {
    let ctx = get_repo_context(git_dir_or_worktree)?;
    let (_, worktree_dir, _) = resolve_git_paths(git_dir_or_worktree)?;
    let repo = gix::open(&worktree_dir)
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

/// Expand and validate relative worktree paths under `worktree_dir`.
pub fn collect_worktree_paths(worktree_dir: &Path, raw_paths: &[String]) -> Result<Vec<String>> {
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

fn hash_worktree_entries(
    snapshot_repo: &gix::Repository,
    worktree_dir: &Path,
) -> Result<BTreeMap<String, gix::ObjectId>> {
    let mut worktree_entries = BTreeMap::new();
    let worktree_files = collect_worktree_paths(worktree_dir, &[".".to_owned()])?;
    for rel_path in worktree_files {
        let full_path = worktree_dir.join(&rel_path);
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
    Ok(worktree_entries)
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

fn delete_ref(repo: &gix::Repository, ref_name: &str) -> Result<()> {
    repo.edit_reference(RefEdit {
        change: Change::Delete {
            expected: PreviousValue::Any,
            log: RefLog::AndReference,
        },
        name: ref_name
            .try_into()
            .map_err(|err| Error::Git(format!("invalid ref name '{ref_name}': {err}")))?,
        deref: false,
    })
    .map_err(|err| Error::Git(format!("failed to delete ref '{ref_name}': {err}")))?;
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
    author_name: &str,
    author_email: &str,
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
        name: author_name.to_owned().into(),
        email: author_email.to_owned().into(),
        time: gix::date::Time::new(timestamp, 0),
    };
    let commit = Commit {
        tree: tree_oid,
        parents: parents.into(),
        author: sig.clone(),
        committer: sig,
        encoding: None,
        message: message.trim_end_matches('\n').to_owned().into(),
        extra_headers: Vec::new(),
    };
    repo.write_object(&commit)
        .map(|id| id.detach())
        .map_err(|err| Error::Git(format!("failed to write commit object: {err}")))
}

fn read_blob_string(repo: &gix::Repository, blob_oid: gix::ObjectId) -> Result<String> {
    let object = repo
        .find_object(blob_oid)
        .map_err(|err| Error::Git(format!("failed to find blob '{blob_oid}': {err}")))?;
    Ok(String::from_utf8_lossy(&object.data).into_owned())
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

fn normalize_commit_message(message: Option<&str>, fallback: &str) -> String {
    message
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}
