//! Typed Git operations shared by snapshot capture and transient diff browsing.
use crate::error::{Result, invalid};
use gix::{
    bstr::{BString, ByteSlice},
    objs::tree::EntryKind,
};
use std::{collections::BTreeSet, path::Path};

/// Enumerate saved working files using Git's index and ignore rules, without changing the index.
pub fn get_worktree_paths(repo: &gix::Repository) -> Result<BTreeSet<BString>> {
    let index = repo.index_or_empty()?;
    let options = repo
        .dirwalk_options()?
        .emit_tracked(true)
        .emit_untracked(gix::dir::walk::EmissionMode::Matching);
    let mut paths = BTreeSet::new();
    for item in repo.dirwalk_iter(
        index,
        std::iter::empty::<BString>(),
        Default::default(),
        options,
    )? {
        let entry = item?.entry;
        if matches!(
            entry.status,
            gix::dir::entry::Status::Tracked | gix::dir::entry::Status::Untracked
        ) && matches!(
            entry.disk_kind,
            Some(
                gix::dir::entry::Kind::File
                    | gix::dir::entry::Kind::Symlink
                    | gix::dir::entry::Kind::Repository
            )
        ) {
            paths.insert(entry.rela_path);
        }
    }
    Ok(paths)
}

/// Build a tree from the source index. Only the destination object database is written.
pub fn write_index_tree(
    destination: &gix::Repository,
    source: &gix::Repository,
) -> Result<gix::ObjectId> {
    let index = source.index_or_empty()?;
    let mut tree = destination.edit_tree(destination.empty_tree().id)?;
    for entry in index.entries() {
        if entry.stage() != gix::index::entry::Stage::Unconflicted {
            return Err(invalid("Staged snapshot requires a resolved index"));
        }
        let mode = entry
            .mode
            .to_tree_entry_mode()
            .ok_or_else(|| invalid("Unsupported index entry mode"))?;
        let id = if mode.is_blob() || mode.is_link() {
            destination
                .write_blob(&source.find_blob(entry.id)?.data)?
                .detach()
        } else {
            entry.id
        };
        tree.upsert(entry.path(&index), mode.kind(), id)?;
    }
    Ok(tree.write()?.detach())
}

/// Write literal saved bytes as Git blobs. Tree construction and path encoding belong to gitoxide.
pub fn write_files(
    destination: &gix::Repository,
    root: &Path,
    paths: impl IntoIterator<Item = impl AsRef<[u8]>>,
) -> Result<gix::ObjectId> {
    let mut tree = destination.edit_tree(destination.empty_tree().id)?;
    for path in paths {
        let path = path.as_ref().as_bstr();
        let selected = root.join(gix::path::from_bstr(path)?);
        let metadata = std::fs::symlink_metadata(&selected)?;
        let (kind, id) = if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(&selected)?;
            (
                EntryKind::Link,
                destination
                    .write_blob(target.as_os_str().as_encoded_bytes())?
                    .detach(),
            )
        } else if metadata.is_file() {
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            let kind = if executable {
                EntryKind::BlobExecutable
            } else {
                EntryKind::Blob
            };
            (
                kind,
                destination
                    .write_blob_stream(std::fs::File::open(&selected)?)?
                    .detach(),
            )
        } else if metadata.is_dir() {
            (
                EntryKind::Commit,
                gix::discover(&selected)?.head_id()?.detach(),
            )
        } else {
            continue;
        };
        tree.upsert(path, kind, id)?;
    }
    Ok(tree.write()?.detach())
}

/// Import original commits into an owned store through gitoxide's fetch transport.
pub fn fetch_commits(repo: &gix::Repository, url: &str, commits: &[String]) -> Result<()> {
    let missing: Vec<_> = commits
        .iter()
        .filter(|sha| gix::ObjectId::from_hex(sha.as_bytes()).is_ok_and(|id| !repo.has_object(id)))
        .cloned()
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let remote = repo.remote_at(url)?.with_refspecs(
        missing.iter().map(|sha| sha.as_bytes().as_bstr()),
        gix::remote::Direction::Fetch,
    )?;
    remote
        .connect(gix::remote::Direction::Fetch)?
        .prepare_fetch(gix::progress::Discard, Default::default())?
        .receive(
            gix::progress::Discard,
            &std::sync::atomic::AtomicBool::new(false),
        )?;
    for sha in missing {
        repo.find_commit(
            gix::ObjectId::from_hex(sha.as_bytes())
                .map_err(|_| invalid("Expected a full commit identity"))?,
        )?;
    }
    Ok(())
}
