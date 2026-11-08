//! Desktop diff service resolving commit checklists, changed file trees, and raw `(base, head)` file contents across `<workspace>/.git`, `~/.lgtm/snapshots/$repoSlug`, and the live working tree.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use gix::bstr::ByteSlice;
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};

use crate::{
    error::{Result, error},
    models::{
        DiffCommitItem, DiffCommitSource, DiffFileEntry, DiffFileStatus, DiffWorkspaceSnapshot,
        FileDiffContent, ReviewMode,
    },
};

const MAX_COMMIT_HISTORY: usize = 64;

struct RepoHandles {
    worktree_dir: PathBuf,
    branch: String,
    head_oid: Option<gix::ObjectId>,
    user_name: String,
    git_repo: gix::Repository,
    snapshot_repo: Option<gix::Repository>,
}

/// Resolves workspace commit history, file-level diff summaries, and raw `(base, head)` file contents from `.git` and `~/.lgtm/snapshots/$repoSlug`.
#[derive(Default)]
pub struct DiffService;

impl DiffService {
    /// Creates a stateless desktop diff service.
    pub fn new() -> Self {
        Self
    }

    // Public ------------------------------------------------------------------

    /// Computes the deterministic `$repoSlug` (`<repo-name>-<6-byte-sha256(normalized_common_git_dir)>`) matching `lgtm-forge`.
    pub fn compute_repo_slug(git_dir_or_worktree: &Path) -> Result<String> {
        let (_, worktree_dir, normalized_git_dir) = resolve_git_paths(git_dir_or_worktree)?;
        let common_path = Path::new(&normalized_git_dir);
        let repo_root = if common_path.file_name().is_some_and(|name| name == ".git") {
            common_path.parent().unwrap_or(&worktree_dir)
        } else {
            common_path
        };
        let repo_name = repo_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("repo")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '-'
                }
            })
            .collect::<String>();

        let full_digest = format!("{:x}", Sha256::digest(normalized_git_dir.as_bytes()));
        let short_hash = &full_digest[..12];
        Ok(format!("{repo_name}-{short_hash}"))
    }

    /// Returns the workspace diff state (branch, commit checklist, default selection, and changed files) for `mode`.
    pub fn get_state(
        &self,
        workspace_root: &Path,
        lgtm_home: &Path,
        mode: ReviewMode,
    ) -> Result<DiffWorkspaceSnapshot> {
        let Some(handles) = open_repo_handles(workspace_root, lgtm_home)? else {
            return Ok(DiffWorkspaceSnapshot {
                review_mode: mode,
                branch: String::new(),
                commits: Vec::new(),
                selected_commit_ids: Vec::new(),
                files: Vec::new(),
            });
        };

        let commits = build_commit_checklist(&handles, mode)?;
        let selected_commit_ids = commits
            .first()
            .map(|first| vec![first.id.clone()])
            .unwrap_or_default();
        let files = if selected_commit_ids.is_empty() {
            Vec::new()
        } else {
            let all_commits = build_all_known_commits(&handles)?;
            let (base_sha, head_sha) = resolve_range(&handles, &all_commits, &selected_commit_ids)?;
            compute_diff_files(&handles, &base_sha, &head_sha)?
        };

        Ok(DiffWorkspaceSnapshot {
            review_mode: mode,
            branch: handles.branch,
            commits,
            selected_commit_ids,
            files,
        })
    }

    /// Returns changed files and their `+additions` / `-deletions` counts across `commit_ids`.
    pub fn get_files(
        &self,
        workspace_root: &Path,
        lgtm_home: &Path,
        commit_ids: &[String],
    ) -> Result<Vec<DiffFileEntry>> {
        if commit_ids.is_empty() {
            return Ok(Vec::new());
        }
        let Some(handles) = open_repo_handles(workspace_root, lgtm_home)? else {
            return Ok(Vec::new());
        };
        let all_commits = build_all_known_commits(&handles)?;
        let (base_sha, head_sha) = resolve_range(&handles, &all_commits, commit_ids)?;
        compute_diff_files(&handles, &base_sha, &head_sha)
    }

    /// Returns the raw `base_content` and `head_content` of `path` across `commit_ids`.
    pub fn get_file_diff(
        &self,
        workspace_root: &Path,
        lgtm_home: &Path,
        path: &str,
        commit_ids: &[String],
    ) -> Result<FileDiffContent> {
        let trimmed_path = path.trim();
        if trimmed_path.is_empty() {
            return Err(error("invalidName", "Choose a file inside the workspace."));
        }
        let Some(handles) = open_repo_handles(workspace_root, lgtm_home)? else {
            return Err(error(
                "unsupported",
                "Active workspace is not a Git repository.",
            ));
        };
        let all_commits = build_all_known_commits(&handles)?;
        let (base_sha, head_sha) = resolve_range(&handles, &all_commits, commit_ids)?;
        let entries = compute_diff_files(&handles, &base_sha, &head_sha)?;
        let matched_entry = entries.iter().find(|entry| entry.path == trimmed_path);
        let status = matched_entry
            .map(|entry| entry.status)
            .unwrap_or(DiffFileStatus::Modified);
        let old_path = matched_entry.and_then(|entry| entry.old_path.clone());

        let base_tree = read_revision_bytes(&handles, &base_sha)?;
        let head_tree = read_revision_bytes(&handles, &head_sha)?;
        let base_lookup_path = old_path.as_deref().unwrap_or(trimmed_path);

        let base_bytes = base_tree.get(base_lookup_path).cloned().unwrap_or_default();
        let head_bytes = head_tree.get(trimmed_path).cloned().unwrap_or_default();

        let base_text = String::from_utf8(base_bytes);
        let head_text = String::from_utf8(head_bytes);
        let is_binary = base_text.is_err() || head_text.is_err();

        Ok(FileDiffContent {
            path: trimmed_path.to_string(),
            old_path,
            status,
            base_sha,
            head_sha: head_sha.clone(),
            base_content: base_text.unwrap_or_default(),
            head_content: head_text.unwrap_or_default(),
            is_binary,
            readonly: head_sha != "working_tree",
        })
    }
}

// Private ---------------------------------------------------------------------

fn open_repo_handles(workspace_root: &Path, lgtm_home: &Path) -> Result<Option<RepoHandles>> {
    if !workspace_root.exists() {
        return Ok(None);
    }
    let Ok((worktree_git_dir, worktree_dir, _common_git_dir)) = resolve_git_paths(workspace_root)
    else {
        return Ok(None);
    };
    let git_repo = match gix::open(&worktree_dir) {
        Ok(repo) => repo,
        Err(_) => match gix::open(&worktree_git_dir) {
            Ok(repo) => repo,
            Err(_) => return Ok(None),
        },
    };

    let branch = git_repo
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_str().unwrap_or("HEAD").to_owned())
        .unwrap_or_else(|| "HEAD".to_owned());

    let head_oid = git_repo.head_id().ok().map(|id| id.detach());
    let config = git_repo.config_snapshot();
    let user_name = config
        .string("user.name")
        .and_then(|v| v.to_str().ok().map(str::trim).map(str::to_owned))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "LGTM Reviewer".to_owned());

    let slug = DiffService::compute_repo_slug(&worktree_dir)?;
    let snapshot_dir = lgtm_home.join("snapshots").join(slug);
    let snapshot_repo = if snapshot_dir.join("HEAD").exists() {
        gix::open(&snapshot_dir).ok()
    } else {
        None
    };

    Ok(Some(RepoHandles {
        worktree_dir,
        branch,
        head_oid,
        user_name,
        git_repo,
        snapshot_repo,
    }))
}

fn build_all_known_commits(handles: &RepoHandles) -> Result<Vec<DiffCommitItem>> {
    let mut items = Vec::new();
    let mut visited = BTreeSet::new();
    let (snapshot_items, git_start_oid) = collect_local_snapshot_commits(handles, &mut visited);
    items.extend(snapshot_items);
    if let Some(head_oid) = handles.head_oid {
        items.extend(collect_git_commits(
            &handles.git_repo,
            head_oid,
            &mut visited,
        ));
    }
    if let Some(start_oid) = git_start_oid {
        items.extend(collect_git_commits(
            &handles.git_repo,
            start_oid,
            &mut visited,
        ));
    }
    let worktree_base_sha = items
        .first()
        .map(|first| first.id.clone())
        .or_else(|| handles.head_oid.map(|oid| oid.to_hex().to_string()))
        .unwrap_or_default();
    let latest_ts = items
        .first()
        .map(|first| first.timestamp + 1)
        .unwrap_or(1_700_000_000);
    items.insert(
        0,
        DiffCommitItem {
            id: "working_tree".to_string(),
            short_sha: "WORKTREE".to_string(),
            title: "Working Tree".to_string(),
            author: handles.user_name.clone(),
            timestamp: latest_ts,
            parent_sha: if worktree_base_sha.is_empty() {
                None
            } else {
                Some(worktree_base_sha)
            },
            source: DiffCommitSource::WorkingTree,
        },
    );
    Ok(items)
}

fn build_commit_checklist(handles: &RepoHandles, mode: ReviewMode) -> Result<Vec<DiffCommitItem>> {
    let mut items = Vec::new();
    let mut visited = BTreeSet::new();

    let (snapshot_items, git_start_oid) = if mode == ReviewMode::Local {
        collect_local_snapshot_commits(handles, &mut visited)
    } else {
        (Vec::new(), handles.head_oid)
    };

    items.extend(snapshot_items);

    if let Some(start_oid) = git_start_oid {
        let git_items = collect_git_commits(&handles.git_repo, start_oid, &mut visited);
        items.extend(git_items);
    }

    if mode == ReviewMode::Local {
        let worktree_base_sha = items
            .first()
            .map(|first| first.id.clone())
            .or_else(|| handles.head_oid.map(|oid| oid.to_hex().to_string()))
            .unwrap_or_default();
        let worktree_changes = compute_diff_files(handles, &worktree_base_sha, "working_tree")?;
        if !worktree_changes.is_empty() {
            let latest_ts = items
                .first()
                .map(|first| first.timestamp + 1)
                .unwrap_or(1_700_000_000);
            items.insert(
                0,
                DiffCommitItem {
                    id: "working_tree".to_string(),
                    short_sha: "WORKTREE".to_string(),
                    title: "Working Tree".to_string(),
                    author: handles.user_name.clone(),
                    timestamp: latest_ts,
                    parent_sha: if worktree_base_sha.is_empty() {
                        None
                    } else {
                        Some(worktree_base_sha)
                    },
                    source: DiffCommitSource::WorkingTree,
                },
            );
        }
    }

    Ok(items)
}

fn collect_local_snapshot_commits(
    handles: &RepoHandles,
    visited: &mut BTreeSet<gix::ObjectId>,
) -> (Vec<DiffCommitItem>, Option<gix::ObjectId>) {
    let Some(snapshot_repo) = handles.snapshot_repo.as_ref() else {
        return (Vec::new(), handles.head_oid);
    };
    let snap_ref = format!("refs/lgtm/snapshots/{}", handles.branch);
    let head_ref = format!("refs/lgtm/heads/{}", handles.branch);
    let Some(mut cursor) = resolve_ref_oid(snapshot_repo, &snap_ref) else {
        return (Vec::new(), handles.head_oid);
    };
    let anchor_oid = resolve_ref_oid(snapshot_repo, &head_ref).or(handles.head_oid);

    let mut items = Vec::new();
    while items.len() < MAX_COMMIT_HISTORY {
        if Some(cursor) == anchor_oid || !visited.insert(cursor) {
            break;
        }
        let Some(item) = decode_commit_item(
            snapshot_repo,
            Some(&handles.git_repo),
            cursor,
            DiffCommitSource::LocalSnapshot,
        ) else {
            break;
        };
        let next_parent = item
            .parent_sha
            .as_deref()
            .and_then(|hex| gix::ObjectId::from_hex(hex.as_bytes()).ok());
        items.push(item);
        let Some(parent_oid) = next_parent else {
            break;
        };
        cursor = parent_oid;
    }

    let git_start = anchor_oid.or(handles.head_oid);
    (items, git_start)
}

fn collect_git_commits(
    git_repo: &gix::Repository,
    start_oid: gix::ObjectId,
    visited: &mut BTreeSet<gix::ObjectId>,
) -> Vec<DiffCommitItem> {
    let mut items = Vec::new();
    let mut cursor = start_oid;
    while items.len() < MAX_COMMIT_HISTORY {
        if !visited.insert(cursor) {
            break;
        }
        let Some(item) = decode_commit_item(git_repo, None, cursor, DiffCommitSource::GitCommit)
        else {
            break;
        };
        let next_parent = item
            .parent_sha
            .as_deref()
            .and_then(|hex| gix::ObjectId::from_hex(hex.as_bytes()).ok());
        items.push(item);
        let Some(parent_oid) = next_parent else {
            break;
        };
        cursor = parent_oid;
    }
    items
}

fn decode_commit_item(
    primary_repo: &gix::Repository,
    fallback_repo: Option<&gix::Repository>,
    commit_oid: gix::ObjectId,
    source: DiffCommitSource,
) -> Option<DiffCommitItem> {
    let object = primary_repo
        .find_object(commit_oid)
        .ok()
        .or_else(|| fallback_repo.and_then(|repo| repo.find_object(commit_oid).ok()))?;
    let commit = object.try_into_commit().ok()?;
    let decoded = commit.decode().ok()?;
    let full_sha = commit_oid.to_hex().to_string();
    let short_sha = full_sha.chars().take(7).collect::<String>();
    let raw_message = decoded.message.to_str_lossy();
    let title = raw_message
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("(no commit message)")
        .to_string();
    let author = decoded.author.name.to_str_lossy().into_owned();
    let timestamp = decoded.author.time.seconds;
    let parent_sha = decoded.parents().next().map(|id| id.to_hex().to_string());

    Some(DiffCommitItem {
        id: full_sha,
        short_sha,
        title,
        author,
        timestamp,
        parent_sha,
        source,
    })
}

fn resolve_range(
    handles: &RepoHandles,
    all_commits: &[DiffCommitItem],
    commit_ids: &[String],
) -> Result<(String, String)> {
    if let Some(range) = DiffCommitItem::resolve_commit_range(all_commits, commit_ids) {
        return Ok(range);
    }
    let newest = commit_ids
        .first()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let oldest = commit_ids
        .last()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if oldest.is_empty() || newest.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let base_sha = if oldest == "working_tree" {
        handles
            .head_oid
            .map(|oid| oid.to_hex().to_string())
            .unwrap_or_default()
    } else if let Ok(oid) = gix::ObjectId::from_hex(oldest.as_bytes()) {
        decode_commit_item(
            &handles.git_repo,
            handles.snapshot_repo.as_ref(),
            oid,
            DiffCommitSource::GitCommit,
        )
        .and_then(|item| item.parent_sha)
        .unwrap_or_default()
    } else {
        String::new()
    };
    Ok((base_sha, newest))
}

fn compute_diff_files(
    handles: &RepoHandles,
    base_sha: &str,
    head_sha: &str,
) -> Result<Vec<DiffFileEntry>> {
    if base_sha == head_sha && !head_sha.is_empty() {
        return Ok(Vec::new());
    }
    let base_files = read_revision_bytes(handles, base_sha)?;
    let head_files = read_revision_bytes(handles, head_sha)?;

    let mut modified_entries = Vec::new();
    let mut removed_paths = Vec::new();
    let mut added_paths = Vec::new();

    let all_paths: BTreeSet<&String> = base_files.keys().chain(head_files.keys()).collect();
    for path in all_paths {
        match (base_files.get(path), head_files.get(path)) {
            (Some(base_bytes), Some(head_bytes)) => {
                if base_bytes != head_bytes {
                    let (additions, deletions) = count_bytes_delta(base_bytes, head_bytes);
                    modified_entries.push(DiffFileEntry {
                        path: path.clone(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions,
                        deletions,
                    });
                }
            }
            (Some(_), None) => removed_paths.push(path.clone()),
            (None, Some(_)) => added_paths.push(path.clone()),
            (None, None) => {}
        }
    }

    let mut matched_removed = BTreeSet::new();
    let mut matched_added = BTreeSet::new();
    let mut renamed_entries = Vec::new();

    // Pass 1: exact byte match renames
    for added in &added_paths {
        let Some(head_bytes) = head_files.get(added) else {
            continue;
        };
        if head_bytes.is_empty() {
            continue;
        }
        if let Some(removed) = removed_paths.iter().find(|rem| {
            !matched_removed.contains(*rem)
                && base_files
                    .get(*rem)
                    .is_some_and(|base_bytes| base_bytes == head_bytes)
        }) {
            matched_removed.insert(removed.clone());
            matched_added.insert(added.clone());
            renamed_entries.push(DiffFileEntry {
                path: added.clone(),
                old_path: Some(removed.clone()),
                status: DiffFileStatus::Renamed,
                additions: 0,
                deletions: 0,
            });
        }
    }

    // Pass 2: similar line content renames (>= 50% unchanged lines)
    for added in &added_paths {
        if matched_added.contains(added) {
            continue;
        }
        let Some(head_bytes) = head_files.get(added) else {
            continue;
        };
        let Ok(head_text) = std::str::from_utf8(head_bytes) else {
            continue;
        };
        if head_text.lines().count() < 2 {
            continue;
        }
        let mut best_match: Option<(String, u32, u32, usize)> = None;
        for removed in &removed_paths {
            if matched_removed.contains(removed) {
                continue;
            }
            let Some(base_bytes) = base_files.get(removed) else {
                continue;
            };
            let Ok(base_text) = std::str::from_utf8(base_bytes) else {
                continue;
            };
            let (equal_lines, additions, deletions) = count_line_similarity(base_text, head_text);
            let max_lines = base_text.lines().count().max(head_text.lines().count());
            if max_lines > 0
                && equal_lines * 2 >= max_lines
                && best_match
                    .as_ref()
                    .is_none_or(|(_, _, _, best_eq)| equal_lines > *best_eq)
            {
                best_match = Some((removed.clone(), additions, deletions, equal_lines));
            }
        }
        if let Some((removed, additions, deletions, _)) = best_match {
            matched_removed.insert(removed.clone());
            matched_added.insert(added.clone());
            renamed_entries.push(DiffFileEntry {
                path: added.clone(),
                old_path: Some(removed),
                status: DiffFileStatus::Renamed,
                additions,
                deletions,
            });
        }
    }

    let mut entries = modified_entries;
    entries.extend(renamed_entries);

    for added in added_paths {
        if matched_added.contains(&added) {
            continue;
        }
        let head_bytes = head_files.get(&added).map(Vec::as_slice).unwrap_or(&[]);
        let (additions, _) = count_bytes_delta(&[], head_bytes);
        entries.push(DiffFileEntry {
            path: added,
            old_path: None,
            status: DiffFileStatus::Added,
            additions,
            deletions: 0,
        });
    }

    for removed in removed_paths {
        if matched_removed.contains(&removed) {
            continue;
        }
        let base_bytes = base_files.get(&removed).map(Vec::as_slice).unwrap_or(&[]);
        let (_, deletions) = count_bytes_delta(base_bytes, &[]);
        entries.push(DiffFileEntry {
            path: removed,
            old_path: None,
            status: DiffFileStatus::Deleted,
            additions: 0,
            deletions,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn count_bytes_delta(base_bytes: &[u8], head_bytes: &[u8]) -> (u32, u32) {
    let Ok(base_text) = std::str::from_utf8(base_bytes) else {
        return (0, 0);
    };
    let Ok(head_text) = std::str::from_utf8(head_bytes) else {
        return (0, 0);
    };
    let (_, additions, deletions) = count_line_similarity(base_text, head_text);
    (additions, deletions)
}

fn count_line_similarity(base_text: &str, head_text: &str) -> (usize, u32, u32) {
    let diff = TextDiff::from_lines(base_text, head_text);
    let mut equal = 0usize;
    let mut additions = 0u32;
    let mut deletions = 0u32;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => equal += 1,
            ChangeTag::Insert => additions += 1,
            ChangeTag::Delete => deletions += 1,
        }
    }
    (equal, additions, deletions)
}

fn read_revision_bytes(handles: &RepoHandles, rev_id: &str) -> Result<BTreeMap<String, Vec<u8>>> {
    let trimmed = rev_id.trim();
    if trimmed.is_empty() {
        return Ok(BTreeMap::new());
    }
    if trimmed == "working_tree" {
        let mut files = BTreeMap::new();
        collect_worktree_files(&handles.worktree_dir, &handles.worktree_dir, &mut files)?;
        return Ok(files);
    }

    let oid = gix::ObjectId::from_hex(trimmed.as_bytes()).map_err(|err| {
        error(
            "invalidRevision",
            format!("Invalid commit SHA '{trimmed}': {err}"),
        )
    })?;

    if handles.git_repo.find_object(oid).is_ok() {
        return read_commit_blobs(&handles.git_repo, handles.snapshot_repo.as_ref(), oid);
    }
    if let Some(snapshot_repo) = handles.snapshot_repo.as_ref()
        && snapshot_repo.find_object(oid).is_ok()
    {
        return read_commit_blobs(snapshot_repo, Some(&handles.git_repo), oid);
    }

    Err(error(
        "notFound",
        format!("Commit '{trimmed}' was not found in repository or snapshot store."),
    ))
}

fn read_commit_blobs(
    primary_repo: &gix::Repository,
    fallback_repo: Option<&gix::Repository>,
    commit_oid: gix::ObjectId,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let object = find_object_any(primary_repo, fallback_repo, commit_oid)?;
    let commit = object.try_into_commit().map_err(|err| {
        error(
            "git",
            format!("Object '{commit_oid}' is not a commit: {err}"),
        )
    })?;
    let tree_oid = commit
        .tree_id()
        .map_err(|err| error("git", format!("Failed to read commit tree: {err}")))?
        .detach();
    let mut files = BTreeMap::new();
    collect_tree_blobs(primary_repo, fallback_repo, tree_oid, "", &mut files)?;
    Ok(files)
}

fn collect_tree_blobs(
    primary_repo: &gix::Repository,
    fallback_repo: Option<&gix::Repository>,
    tree_oid: gix::ObjectId,
    prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let object = find_object_any(primary_repo, fallback_repo, tree_oid)?;
    let tree = object
        .try_into_tree()
        .map_err(|err| error("git", format!("Object '{tree_oid}' is not a tree: {err}")))?;
    let decoded = tree
        .decode()
        .map_err(|err| error("git", format!("Failed to decode tree '{tree_oid}': {err}")))?;
    for entry in &decoded.entries {
        let Ok(name) = entry.filename.to_str() else {
            continue;
        };
        if name == ".lgtm" {
            continue;
        }
        let rel_path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let entry_oid: gix::ObjectId = entry.oid.into();
        if entry.mode.is_tree() {
            collect_tree_blobs(primary_repo, fallback_repo, entry_oid, &rel_path, out)?;
        } else if entry.mode.is_blob() || entry.mode.is_executable() {
            let blob_obj = find_object_any(primary_repo, fallback_repo, entry_oid)?;
            out.insert(rel_path, blob_obj.data.clone());
        }
    }
    Ok(())
}

fn find_object_any<'a>(
    primary_repo: &'a gix::Repository,
    fallback_repo: Option<&'a gix::Repository>,
    oid: gix::ObjectId,
) -> Result<gix::Object<'a>> {
    if let Ok(obj) = primary_repo.find_object(oid) {
        return Ok(obj);
    }
    if let Some(fallback) = fallback_repo
        && let Ok(obj) = fallback.find_object(oid)
    {
        return Ok(obj);
    }
    Err(error("notFound", format!("Git object '{oid}' not found.")))
}

fn collect_worktree_files(
    worktree_root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        if name_str == ".git"
            || name_str == ".lgtm"
            || name_str == "target"
            || name_str == "node_modules"
            || name_str.starts_with(".lgtm-save-")
        {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_worktree_files(worktree_root, &path, out)?;
        } else if meta.is_file()
            && let Ok(rel) = path.strip_prefix(worktree_root)
        {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(&path)?;
            out.insert(rel_str, bytes);
        }
    }
    Ok(())
}

fn resolve_ref_oid(repo: &gix::Repository, ref_name: &str) -> Option<gix::ObjectId> {
    repo.find_reference(ref_name)
        .ok()
        .and_then(|mut reference| reference.peel_to_id_in_place().ok().map(|id| id.detach()))
}

fn resolve_git_paths(git_dir_or_worktree: &Path) -> Result<(PathBuf, PathBuf, String)> {
    let canonical = std::fs::canonicalize(git_dir_or_worktree)?;
    let (worktree_git_dir, worktree_dir) =
        if canonical.file_name().is_some_and(|name| name == ".git") {
            let worktree = canonical
                .parent()
                .ok_or_else(|| error("notGitRepo", "Git directory has no parent worktree."))?
                .to_path_buf();
            let resolved_git = if canonical.is_file() {
                resolve_gitdir_file(&canonical)?
            } else {
                canonical
            };
            (resolved_git, worktree)
        } else if canonical.join(".git").exists() {
            let dot_git = canonical.join(".git");
            let resolved_git = if dot_git.is_file() {
                resolve_gitdir_file(&dot_git)?
            } else {
                dot_git
            };
            (resolved_git, canonical)
        } else if canonical.join("gitdir").is_file() && canonical.join("commondir").is_file() {
            let wt_git_file = std::fs::read_to_string(canonical.join("gitdir"))?;
            let wt_dot_git = Path::new(wt_git_file.trim());
            let wt_dir = wt_dot_git
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| canonical.clone());
            (canonical, wt_dir)
        } else {
            return Err(error(
                "notGitRepo",
                "Active workspace is not a Git repository.",
            ));
        };

    let common_git_dir = resolve_common_git_dir(&worktree_git_dir)?;
    let normalized_git_dir = common_git_dir.to_string_lossy().into_owned();
    Ok((worktree_git_dir, worktree_dir, normalized_git_dir))
}

fn resolve_common_git_dir(worktree_git_dir: &Path) -> Result<PathBuf> {
    let commondir_file = worktree_git_dir.join("commondir");
    if commondir_file.is_file() {
        let raw = std::fs::read_to_string(&commondir_file)?;
        let rel = raw.trim();
        let target = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            worktree_git_dir.join(rel)
        };
        return Ok(std::fs::canonicalize(target)?);
    }
    Ok(worktree_git_dir.to_path_buf())
}

fn resolve_gitdir_file(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_file)?;
    let raw_target = contents
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| error("notGitRepo", "Malformed .git file."))?
        .trim();
    let target_path = Path::new(raw_target);
    let resolved = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        dot_git_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target_path)
    };
    Ok(std::fs::canonicalize(resolved)?)
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_git_repository_on_disk {
        use super::*;

        mod when_computing_repo_slug {
            use super::*;

            #[test]
            fn should_produce_deterministic_name_and_12_hex_char_sha256_prefix() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let repo_dir = temp.path().join("my.repo");
                std::fs::create_dir_all(&repo_dir).unwrap();
                gix::init(&repo_dir).unwrap();

                // Act
                let slug_first = DiffService::compute_repo_slug(&repo_dir).unwrap();
                let slug_second = DiffService::compute_repo_slug(&repo_dir).unwrap();

                // Assert
                assert_eq!(slug_first, slug_second);
                assert!(slug_first.starts_with("my-repo-"));
                assert_eq!(slug_first.len(), "my-repo-".len() + 12);
            }
        }
    }
}
