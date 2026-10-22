//! Explicit local content selection. Ignored files are included for notes roots.
use crate::error::{Result, invalid};
use crate::models::{Context, ContextKey, LocalLocation};
use crate::utils::WorkspaceFile;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Read one saved source without changing the index, checkout, or review records.
/// Index identity comes from stage zero, independently of the working file.
pub fn get_source(
    root: &Path,
    path: &str,
    kind: crate::database::types::SourceKind,
    commit: Option<&str>,
) -> Result<(crate::models::AnchorSource, Vec<u8>)> {
    use crate::{
        database::types::SourceKind,
        models::{Anchor, AnchorSource},
    };
    Anchor::File { path: path.into() }.validate()?;
    let root = root.canonicalize()?;
    let git = |args: &[&str]| -> Result<Vec<u8>> {
        let result = std::process::Command::new("git")
            .arg("--literal-pathspecs")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()?;
        if !result.status.success() {
            return Err(invalid("Selected Git source is unavailable"));
        }
        Ok(result.stdout)
    };
    let (oid, sha) = match kind {
        SourceKind::IndexFile => {
            let entries = git(&["ls-files", "--stage", "-z", "--", path])?;
            let entries: Vec<_> = entries
                .split(|b| *b == 0)
                .filter(|s| !s.is_empty())
                .collect();
            if entries.len() != 1 {
                return Err(invalid("Selected file requires one stage-zero index entry"));
            }
            let entry =
                std::str::from_utf8(entries[0]).map_err(|_| invalid("Invalid index path"))?;
            let (meta, found) = entry
                .split_once('\t')
                .ok_or_else(|| invalid("Invalid index entry"))?;
            let parts: Vec<_> = meta.split_whitespace().collect();
            if found != path
                || parts.len() != 3
                || parts[2] != "0"
                || !["100644", "100755"].contains(&parts[0])
            {
                return Err(invalid(
                    "Selected file requires a regular stage-zero index entry",
                ));
            }
            (Some(parts[1].to_owned()), None)
        }
        SourceKind::Commit => {
            let sha = commit.ok_or_else(|| invalid("Commit source requires its full identity"))?;
            if ![40, 64].contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid("Expected a full commit identity"));
            }
            git(&["rev-parse", "--verify", &format!("{sha}^{{commit}}")])?;
            let entries = git(&["ls-tree", "-z", sha, "--", path])?;
            let entry =
                std::str::from_utf8(&entries).map_err(|_| invalid("Invalid Git tree entry"))?;
            let (meta, found) = entry
                .trim_end_matches('\0')
                .split_once('\t')
                .ok_or_else(|| invalid("Selected committed file is unavailable"))?;
            let parts: Vec<_> = meta.split_whitespace().collect();
            if found != path
                || parts.len() != 3
                || parts[1] != "blob"
                || !["100644", "100755"].contains(&parts[0])
            {
                return Err(invalid("Selected source is not a regular file"));
            }
            (Some(parts[2].to_owned()), Some(sha.to_owned()))
        }
        SourceKind::WorkingFile | SourceKind::LocalFile => (None, None),
        SourceKind::Unavailable => {
            return Err(invalid(
                "Unavailable source cannot establish new anchor evidence",
            ));
        }
    };
    let bytes = if let Some(oid) = &oid {
        let size = git(&["cat-file", "-s", oid])?;
        if std::str::from_utf8(&size)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .is_none_or(|n| n > 2 * 1024 * 1024)
        {
            return Err(invalid("Selected source exceeds the text size limit"));
        }
        git(&["cat-file", "blob", oid])?
    } else {
        let mut candidate = root.clone();
        for component in Path::new(path).components() {
            candidate.push(component);
            if std::fs::symlink_metadata(&candidate)?
                .file_type()
                .is_symlink()
            {
                return Err(invalid("Selected source crosses a nested symlink"));
            }
        }
        let file = root.join(path).canonicalize()?;
        if !file.starts_with(&root) || !file.is_file() {
            return Err(invalid("Selected source escapes its root or is not a file"));
        }
        if file.metadata()?.len() > 2 * 1024 * 1024 {
            return Err(invalid("Selected source exceeds the text size limit"));
        }
        std::fs::read(file)?
    };
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return Err(invalid("Anchor source requires UTF-8 text"));
    }
    let content_hash = AnchorSource::hash(&bytes);
    let source = match kind {
        SourceKind::Commit => AnchorSource::Commit {
            sha: sha.unwrap(),
            content_hash,
        },
        SourceKind::IndexFile => AnchorSource::IndexFile {
            blob_oid: oid.unwrap(),
            content_hash,
        },
        SourceKind::WorkingFile => AnchorSource::WorkingFile { content_hash },
        SourceKind::LocalFile => AnchorSource::LocalFile { content_hash },
        SourceKind::Unavailable => unreachable!(),
    };
    Ok((source, bytes))
}

/// Enumerate within the explicitly selected root, including symlinked root directories.
/// Nested symlinks are never followed, so cycles and escapes cannot expose other roots.
pub fn get_workspace(context: &Context, selected: &str) -> Result<(String, Vec<WorkspaceFile>)> {
    let root = match &context.key {
        ContextKey::Local {
            location: LocalLocation::Notes { path },
        } => PathBuf::from(context.current_notes_path.as_ref().unwrap_or(path)),
        ContextKey::Local {
            location: LocalLocation::Branch { git_dir, .. },
        } => {
            let output = std::process::Command::new("git")
                .args(["--git-dir", git_dir, "rev-parse", "--show-toplevel"])
                .output()?;
            if !output.status.success() {
                return Err(invalid("Cannot find Git working tree"));
            }
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        }
        ContextKey::ForgeBacked { .. } => context
            .paths
            .first()
            .map(PathBuf::from)
            .ok_or_else(|| invalid("Associate a workspace path before reading local content"))?,
    };
    let root = root.canonicalize()?;
    let boundary = if root.is_file() {
        root.parent().unwrap().to_path_buf()
    } else {
        root.clone()
    };
    let target = if selected.is_empty() {
        root.clone()
    } else {
        boundary.join(selected).canonicalize()?
    };
    if root.is_file() && target != root {
        return Err(invalid(
            "A file-scoped notes context cannot read sibling content",
        ));
    }
    if !target.starts_with(&boundary) {
        return Err(invalid("Selected path escapes the workspace root"));
    }
    let mut files = Vec::new();
    collect(&target, &boundary, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((boundary.to_string_lossy().into_owned(), files))
}

fn collect(path: &Path, root: &Path, files: &mut Vec<WorkspaceFile>) -> Result<()> {
    if files.len() >= 5000 {
        return Err(invalid(
            "Workspace exceeds 5000 files; select a narrower directory",
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }
            collect(&entry.path(), root, files)?;
        }
    } else if metadata.is_file() {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid("Path outside root"))?
            .to_string_lossy()
            .replace('\\', "/");
        if metadata.len() > 2 * 1024 * 1024 {
            files.push(WorkspaceFile {
                previous_path: String::new(),
                path: relative,
                content: String::new(),
                content_hash: String::new(),
                status: "too_large".into(),
            });
            return Ok(());
        }
        let bytes = std::fs::read(path)?;
        let hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        match String::from_utf8(bytes) {
            Ok(content) if !content.contains('\0') => files.push(WorkspaceFile {
                previous_path: String::new(),
                path: relative,
                content,
                content_hash: hash,
                status: "present".into(),
            }),
            _ => files.push(WorkspaceFile {
                previous_path: String::new(),
                path: relative,
                content: String::new(),
                content_hash: hash,
                status: "binary".into(),
            }),
        }
    }
    Ok(())
}

/// Select Git working/index/revision content without modifying the working tree.
pub fn get_review_workspace(
    context: &Context,
    selected: &str,
    mode: &str,
    base: &str,
    head: &str,
) -> Result<(String, Vec<(WorkspaceFile, String)>)> {
    if context.key.is_notes() {
        if !mode.is_empty() && mode != "directory" {
            return Err(invalid(
                "Git content modes require a branch or forge-backed context",
            ));
        }
        let (root, files) = get_workspace(context, selected)?;
        return Ok((
            root,
            files.into_iter().map(|f| (f, String::new())).collect(),
        ));
    }
    let root = if let Some(path) = context.paths.first() {
        PathBuf::from(path).canonicalize()?
    } else if let Some(git_dir) = context.key.git_dir() {
        let gd = PathBuf::from(git_dir).canonicalize()?;
        if gd.file_name().is_some_and(|p| p == ".git") {
            gd.parent().unwrap().to_path_buf()
        } else {
            return Err(invalid(
                "Associate the Git working tree path with this context",
            ));
        }
    } else {
        return Err(invalid("Associate a local checkout to view Git content"));
    };
    let mode = if mode.is_empty() { "working" } else { mode };
    if !["working", "staged", "revisions"].contains(&mode) {
        return Err(invalid(
            "Expected working, staged or revisions workspace mode",
        ));
    }
    fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(invalid(
                "Git content operation failed; check checkout and revisions",
            ));
        }
        Ok(output.stdout)
    }
    fn revision(root: &Path, value: &str) -> Result<String> {
        let value = if value.is_empty() { "HEAD" } else { value };
        if value.starts_with('-') {
            return Err(invalid("Invalid Git revision"));
        }
        let resolved = git(
            root,
            &["rev-parse", "--verify", &format!("{value}^{{commit}}")],
        )?;
        Ok(String::from_utf8_lossy(&resolved).trim().to_owned())
    }
    let base = revision(&root, base)?;
    let head = if mode == "revisions" {
        revision(&root, head)?
    } else {
        String::new()
    };
    let names = match mode {
        "staged" => git(
            &root,
            &["diff", "--cached", "--name-only", "-z", &base, "--"],
        )?,
        "revisions" => git(&root, &["diff", "--name-only", "-z", &base, &head, "--"])?,
        _ => git(&root, &["diff", "--name-only", "-z", &base, "--"])?,
    };
    let mut paths: Vec<_> = names
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect();
    if mode == "working" {
        paths.extend(
            git(&root, &["ls-files", "--others", "--exclude-standard", "-z"])?
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| String::from_utf8_lossy(p).into_owned()),
        );
    }
    paths.sort();
    paths.dedup();
    let mut files = Vec::new();
    for path in paths {
        if !selected.is_empty() && !Path::new(&path).starts_with(selected) {
            continue;
        }
        let old = git(&root, &["show", &format!("{base}:{path}")]).unwrap_or_default();
        let current = match mode {
            "staged" => git(&root, &["show", &format!(":{path}")]).unwrap_or_default(),
            "revisions" => git(&root, &["show", &format!("{head}:{path}")]).unwrap_or_default(),
            _ => {
                let file = root.join(&path);
                if let Ok(resolved) = file.canonicalize()
                    && !resolved.starts_with(&root)
                {
                    return Err(invalid("Git path resolves outside working tree"));
                }
                std::fs::read(&file).unwrap_or_default()
            }
        };
        let hash = format!("sha256:{:x}", Sha256::digest(&current));
        let status = if current.len() > 2 * 1024 * 1024 || old.len() > 2 * 1024 * 1024 {
            "too_large"
        } else if current.contains(&0)
            || old.contains(&0)
            || std::str::from_utf8(&current).is_err()
            || std::str::from_utf8(&old).is_err()
        {
            "binary"
        } else {
            "present"
        };
        let (content, base_content) = if status == "present" {
            (
                String::from_utf8(current).unwrap(),
                String::from_utf8(old).unwrap(),
            )
        } else {
            (String::new(), String::new())
        };
        files.push((
            WorkspaceFile {
                previous_path: String::new(),
                path,
                content,
                content_hash: hash,
                status: status.into(),
            },
            base_content,
        ));
    }
    Ok((root.to_string_lossy().into_owned(), files))
}

/// Read explicit working-tree and branch metadata without review persistence.
/// Resolve a Git working tree; detached checkouts are valid content sources.
pub async fn get_git_repository(path: &str) -> Result<(String, String, Option<String>)> {
    if !Path::new(path).is_absolute() {
        return Err(invalid("Git location must be absolute"));
    }
    let path = std::fs::canonicalize(path)?;
    if !path.is_dir() {
        return Err(invalid("Local Git target requires a directory"));
    }
    let read = |args: Vec<&str>| {
        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("-C")
            .arg(&path)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .kill_on_drop(true);
        cmd
    };
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read(vec!["rev-parse", "--show-toplevel", "--absolute-git-dir"]).output(),
    )
    .await
    .map_err(|_| invalid("Git location lookup timed out"))??;
    if !output.status.success() {
        return Err(invalid("Target is not an available Git working tree"));
    }
    let values: Vec<_> = std::str::from_utf8(&output.stdout)
        .map_err(|_| invalid("Git location is not UTF-8"))?
        .lines()
        .map(str::to_owned)
        .collect();
    if values.len() != 2 {
        return Err(invalid("Git location metadata is incomplete"));
    }
    let branch = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read(vec!["symbolic-ref", "-q", "HEAD"]).output(),
    )
    .await
    .map_err(|_| invalid("Git branch lookup timed out"))??;
    let branch = branch
        .status
        .success()
        .then(|| String::from_utf8_lossy(&branch.stdout).trim().to_owned());
    Ok((values[0].clone(), values[1].clone(), branch))
}

pub async fn get_git_location(path: &str) -> Result<(String, String, String)> {
    let (root, git_dir, branch) = get_git_repository(path).await?;
    Ok((
        root,
        git_dir,
        branch.ok_or_else(|| invalid("Local branch review requires a checked-out branch"))?,
    ))
}
