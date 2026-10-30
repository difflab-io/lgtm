//! Literal saved-source reads and local repository discovery through gitoxide.
use crate::error::{Result, invalid};
use gix::bstr::ByteSlice;
use std::path::Path;

/// Check the index or a committed tree without reading file bodies.
pub fn get_source_presence(root: &Path, path: &str, commit: Option<&str>) -> Result<bool> {
    crate::models::Anchor::File { path: path.into() }.validate()?;
    let repo = gix::open(root)?;
    if let Some(commit) = commit {
        let id = gix::ObjectId::from_hex(commit.as_bytes())
            .map_err(|_| invalid("Expected a full commit identity"))?;
        Ok(repo
            .find_commit(id)?
            .tree()?
            .lookup_entry_by_path(path)?
            .is_some())
    } else {
        let index = repo.index_or_empty()?;
        Ok(index
            .entry_by_path_and_stage(
                path.as_bytes().as_bstr(),
                gix::index::entry::Stage::Unconflicted,
            )
            .is_some())
    }
}

/// Read exact saved bytes. The user's index and checkout remain untouched.
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
    let git_source = match kind {
        SourceKind::IndexFile | SourceKind::Commit => {
            let repo = gix::open(&root)?;
            let id = if kind == SourceKind::IndexFile {
                let index = repo.index_or_empty()?;
                let entry = index
                    .entry_by_path_and_stage(
                        path.as_bytes().as_bstr(),
                        gix::index::entry::Stage::Unconflicted,
                    )
                    .ok_or_else(|| {
                        invalid("Selected file requires a regular stage-zero index entry")
                    })?;
                if !entry
                    .mode
                    .to_tree_entry_mode()
                    .is_some_and(|mode| mode.is_blob())
                {
                    return Err(invalid("Selected source is not a regular file"));
                }
                entry.id
            } else {
                let id = gix::ObjectId::from_hex(
                    commit
                        .ok_or_else(|| invalid("Commit source requires its full identity"))?
                        .as_bytes(),
                )
                .map_err(|_| invalid("Expected a full commit identity"))?;
                let entry = repo
                    .find_commit(id)?
                    .tree()?
                    .lookup_entry_by_path(path)?
                    .ok_or_else(|| invalid("Selected committed file is unavailable"))?;
                if !entry.mode().is_blob() {
                    return Err(invalid("Selected source is not a regular file"));
                }
                entry.id().detach()
            };
            let header = repo.find_header(id)?;
            if header.size() > 2 * 1024 * 1024 {
                return Err(invalid("Selected source exceeds the text size limit"));
            }
            Some((id, repo.find_blob(id)?.data.clone()))
        }
        _ => None,
    };
    let (oid, bytes) = if let Some((oid, bytes)) = git_source {
        (Some(oid), bytes)
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
        (None, std::fs::read(file)?)
    };
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return Err(invalid("Anchor source requires UTF-8 text"));
    }
    let content_hash = AnchorSource::hash(&bytes);
    let source = match kind {
        SourceKind::Commit => AnchorSource::Commit {
            sha: commit.unwrap().into(),
            content_hash,
        },
        SourceKind::IndexFile => AnchorSource::IndexFile {
            blob_oid: oid.unwrap().to_string(),
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
    tokio::task::spawn_blocking(move || {
        let repo = gix::discover(&path)?;
        let root = repo
            .workdir()
            .ok_or_else(|| invalid("Target is not an available Git working tree"))?
            .canonicalize()?;
        let git_dir = repo.git_dir().canonicalize()?;
        let branch = repo
            .head_name()?
            .map(|name| {
                name.as_bstr()
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|_| invalid("Git branch is not UTF-8"))
            })
            .transpose()?;
        Ok((
            root.to_string_lossy().into_owned(),
            git_dir.to_string_lossy().into_owned(),
            branch,
        ))
    })
    .await
    .map_err(|_| invalid("Git location lookup failed"))?
}

pub async fn get_git_location(path: &str) -> Result<(String, String, String)> {
    let (root, git_dir, branch) = get_git_repository(path).await?;
    Ok((
        root,
        git_dir,
        branch.ok_or_else(|| invalid("Local branch review requires a checked-out branch"))?,
    ))
}
