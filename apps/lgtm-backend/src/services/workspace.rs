//! Literal Git source reads and local repository discovery. SnapshotService owns saved review source.
use crate::error::{Result, invalid};
use crate::extensions::git;
use std::path::Path;

/// Check the selected index/tree without loading hidden file bodies or consulting the working file.
pub fn get_source_presence(root: &Path, path: &str, commit: Option<&str>) -> Result<bool> {
    crate::models::Anchor::File { path: path.into() }.validate()?;
    let arguments = if let Some(commit) = commit {
        if ![40, 64].contains(&commit.len()) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("Expected a full commit identity"));
        }
        vec!["ls-tree", "-z", commit, "--", path]
    } else {
        vec!["ls-files", "--stage", "-z", "--", path]
    };
    Ok(!git::get_bytes(root, &arguments)?.is_empty())
}

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
    let (oid, sha) = match kind {
        SourceKind::IndexFile => {
            let entries = git::get_bytes(&root, &["ls-files", "--stage", "-z", "--", path])?;
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
            git::get_bytes(
                &root,
                &["rev-parse", "--verify", &format!("{sha}^{{commit}}")],
            )?;
            let entries = git::get_bytes(&root, &["ls-tree", "-z", sha, "--", path])?;
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
    };
    let bytes = if let Some(oid) = &oid {
        let size = git::get_bytes(&root, &["cat-file", "-s", oid])?;
        if std::str::from_utf8(&size)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .is_none_or(|n| n > 2 * 1024 * 1024)
        {
            return Err(invalid("Selected source exceeds the text size limit"));
        }
        git::get_bytes(&root, &["cat-file", "blob", oid])?
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
    };
    Ok((source, bytes))
}

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
