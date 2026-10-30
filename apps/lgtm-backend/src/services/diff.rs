//! Content comparisons and selected-file reads. This service has no review database handle.
use crate::extensions::{gh, git};
use crate::forge::ForgeClient;
use crate::{
    error::{Error, Result, invalid},
    forge::{GithubClient, Repository},
    models::WorkspaceFile,
    models::{
        Anchor, AnchorSource, ContentSide, DiffComparison, DiffContent, DiffFile, ReviewComparison,
        ReviewTarget,
    },
};
use gix::bstr::ByteSlice;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct DiffService {
    provider: Option<GithubClient>,
}

impl DiffService {
    // Public ------------------------------------------------------------------

    // Construction and configuration ------------------------------------------
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(provider: GithubClient) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    // Diffs and saved sources ----------------------------------------------

    /// Read one saved source without enumerating a diff or changing Git.
    pub async fn get_source(
        &self,
        root: &str,
        path: &str,
        kind: crate::database::types::SourceKind,
        commit: Option<&str>,
    ) -> Result<Option<ContentSide>> {
        use crate::database::types::SourceKind;
        Anchor::File { path: path.into() }.validate()?;
        if !Path::new(root).is_absolute() || (kind == SourceKind::Commit) != commit.is_some() {
            return Err(invalid(
                "Saved source requires an absolute root and a commit only for committed content",
            ));
        }
        let root = PathBuf::from(root);
        let path = path.to_owned();
        let commit = commit.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let present = match kind {
                SourceKind::IndexFile | SourceKind::Commit => {
                    super::workspace::get_source_presence(&root, &path, commit.as_deref())?
                }
                _ => match std::fs::symlink_metadata(root.join(&path)) {
                    Ok(_) => true,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error.into()),
                },
            };
            if !present {
                return Ok(None);
            }
            let (mut source, bytes) =
                super::workspace::get_source(&root, &path, kind, commit.as_deref())?;
            if kind == SourceKind::WorkingFile
                && let Ok((indexed, indexed_bytes)) =
                    super::workspace::get_source(&root, &path, SourceKind::IndexFile, None)
                && indexed_bytes == bytes
            {
                source = indexed;
            }
            Ok(Some(ContentSide {
                path,
                source,
                content: String::from_utf8(bytes)
                    .map_err(|_| invalid("Selected source requires UTF-8 text"))?,
            }))
        })
        .await
        .map_err(|_| invalid("Selected source read failed"))?
    }

    pub async fn get_remote_workspace(
        &self,
        repo: &Repository,
        pr: i64,
        expected_base: Option<&str>,
        expected_head: &str,
        selected: &str,
        metadata_only: bool,
    ) -> Result<(String, Vec<(WorkspaceFile, String)>)> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let github = self.get_provider().await?;
        let pull = github.get_pull_request(repo, pr).await?;
        if expected_head != pull.head || expected_base.is_some_and(|base| base != pull.base) {
            return Err(Error::Conflict(
                "PR head changed; refresh before reading content".into(),
            ));
        }
        let files = gh::get_all(
            &github.client,
            &format!("{}/pulls/{pr}/files", repo.route()),
        )
        .await?;
        if files.len() >= 3000 {
            return Err(invalid(
                "GitHub file listing may be truncated; narrow the review before proceeding",
            ));
        }
        let mut output = vec![];
        for file in files {
            let path = file["filename"]
                .as_str()
                .ok_or_else(|| invalid("GitHub file missing path"))?;
            Anchor::File { path: path.into() }.validate()?;
            let status = file["status"]
                .as_str()
                .ok_or_else(|| invalid("GitHub file missing status"))?;
            let previous = file["previous_filename"].as_str().unwrap_or(path);
            Anchor::File {
                path: previous.into(),
            }
            .validate()?;
            if !selected.is_empty() && path != selected {
                continue;
            }
            if metadata_only {
                output.push((
                    WorkspaceFile {
                        path: path.into(),
                        previous_path: previous.into(),
                        content: String::new(),
                        content_hash: format!(
                            "git:{}:{}",
                            pull.head,
                            file["sha"].as_str().unwrap_or(path)
                        ),
                        status: status.into(),
                    },
                    String::new(),
                ));
                continue;
            }
            let mut sides = vec![];
            for (name, revision, absent) in [
                (previous, &pull.base, status == "added"),
                (path, &pull.head, status == "removed"),
            ] {
                if absent {
                    sides.push(String::new());
                    continue;
                }
                let encoded = name
                    .split('/')
                    .map(|part| {
                        url::form_urlencoded::byte_serialize(part.as_bytes())
                            .collect::<String>()
                            .replace('+', "%20")
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let result = github
                    .get(
                        &format!("{}/contents/{encoded}", repo.route()),
                        Some(&json!({"ref":revision})),
                    )
                    .await?;
                if result["size"].as_u64().is_none_or(|n| n > 1024 * 1024) {
                    return Err(invalid("Remote file exceeds supported 1 MiB text limit"));
                }
                if result["encoding"] != "base64" {
                    return Err(invalid("Remote content encoding unsupported"));
                }
                let encoded = result["content"]
                    .as_str()
                    .ok_or_else(|| invalid("GitHub content missing"))?
                    .replace(['\n', '\r'], "");
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| invalid("Malformed GitHub content encoding"))?;
                if bytes.len() > 1024 * 1024 {
                    return Err(invalid("Remote file exceeds supported 1 MiB text limit"));
                }
                if bytes.contains(&0) {
                    return Err(invalid("Binary remote file is unsupported"));
                }
                sides.push(
                    String::from_utf8(bytes)
                        .map_err(|_| invalid("Remote file is not UTF-8 text"))?,
                );
            }
            let content = sides.pop().unwrap();
            let base = sides.pop().unwrap();
            output.push((
                WorkspaceFile {
                    previous_path: previous.into(),
                    path: path.into(),
                    content_hash: format!("sha256:{:x}", Sha256::digest(content.as_bytes())),
                    content,
                    status: status.into(),
                },
                base,
            ));
        }
        let after = github.get_pull_request(repo, pr).await?;
        if after.head != pull.head || after.base != pull.base {
            return Err(Error::Conflict(
                "PR head changed during content fetch".into(),
            ));
        }
        Ok((repo.repository_url.clone(), output))
    }

    /// Resolve content metadata only. This creates no context, session, or comment.
    pub async fn get_diff(
        &self,
        target: ReviewTarget,
        selected: Option<ReviewComparison>,
    ) -> Result<DiffComparison> {
        let (target, comparison, root) = match target {
            ReviewTarget::Directory { path } => {
                if !Path::new(&path).is_absolute() {
                    return Err(invalid("Directory location must be absolute"));
                }
                let root = Path::new(&path).canonicalize()?;
                if !root.is_dir()
                    || selected
                        .as_ref()
                        .is_some_and(|c| *c != ReviewComparison::Directory)
                {
                    return Err(invalid(
                        "Directory comparison requires its explicit directory root",
                    ));
                }
                let root = root.to_string_lossy().into_owned();
                (
                    ReviewTarget::Directory { path: root.clone() },
                    ReviewComparison::Directory,
                    Some(root),
                )
            }
            ReviewTarget::LocalGit { path, branch } => {
                let (root, _, actual_branch) = super::workspace::get_git_location(&path).await?;
                if branch.as_ref().is_some_and(|b| b != &actual_branch) {
                    return Err(Error::Conflict("Selected branch changed".into()));
                }
                let comparison = match selected {
                    Some(
                        value @ (ReviewComparison::Working { .. }
                        | ReviewComparison::Staged { .. }
                        | ReviewComparison::Branch { .. }),
                    ) => value,
                    None => ReviewComparison::Working {
                        base_commit: gix::open(&root)?.head_id()?.to_string(),
                    },
                    _ => {
                        return Err(invalid(
                            "Local Git target requires working, staged, or branch content",
                        ));
                    }
                };
                comparison.validate()?;
                (
                    ReviewTarget::LocalGit {
                        path: root.clone(),
                        branch: Some(actual_branch),
                    },
                    comparison,
                    Some(root),
                )
            }
            ReviewTarget::PullRequest {
                repository_url,
                number,
            } => {
                if number < 1 {
                    return Err(invalid("PR number must be positive"));
                }
                let repo = Repository::from_repository_url(&repository_url)?;
                let pull = self
                    .get_provider()
                    .await?
                    .get_pull_request(&repo, number)
                    .await?;
                let comparison = ReviewComparison::PullRequest {
                    base_commit: pull.base,
                    head_commit: pull.head,
                };
                comparison.validate()?;
                if selected.as_ref().is_some_and(|c| *c != comparison) {
                    return Err(Error::Conflict("PR comparison changed".into()));
                }
                (
                    ReviewTarget::PullRequest {
                        repository_url: repo.repository_url,
                        number,
                    },
                    comparison,
                    None,
                )
            }
        };
        let mut diff = DiffComparison {
            target,
            comparison,
            root,
            observation_token: String::new(),
        };
        let files = self.get_metadata(&diff).await?;
        diff.observation_token = Self::get_observation_token(&diff, &files)?;
        Ok(diff)
    }

    /// Enumerate metadata separately from selected-file bodies, rejecting stale observations.
    pub async fn get_files(&self, diff: &DiffComparison) -> Result<Vec<DiffFile>> {
        let files = self.get_metadata(diff).await?;
        if Self::get_observation_token(diff, &files)? != diff.observation_token {
            return Err(Error::Conflict(
                "Diff sources changed; read fresh comparison metadata".into(),
            ));
        }
        Ok(files)
    }

    /// Read exactly one listed file and its actual sources. No review data is queried.
    pub async fn get_file(&self, diff: &DiffComparison, path: &str) -> Result<DiffContent> {
        Anchor::File { path: path.into() }.validate()?;
        let file = self
            .get_files(diff)
            .await?
            .into_iter()
            .find(|f| f.path == path)
            .ok_or_else(|| Error::NotFound("File in selected comparison".into()))?;
        if file.status == "too_large" {
            return Err(invalid("Selected file exceeds the text size limit"));
        }
        let previous = if file.previous_path.is_empty() {
            &file.path
        } else {
            &file.previous_path
        };
        let (old, new) = if let (
            ReviewTarget::PullRequest {
                repository_url,
                number,
            },
            ReviewComparison::PullRequest {
                base_commit,
                head_commit,
            },
        ) = (&diff.target, &diff.comparison)
        {
            let repo = Repository::from_repository_url(repository_url)?;
            let (_, files) = self
                .get_remote_workspace(&repo, *number, Some(base_commit), head_commit, path, false)
                .await?;
            let (loaded, old) = files
                .into_iter()
                .next()
                .ok_or_else(|| Error::NotFound("Remote file in selected comparison".into()))?;
            let old = (file.status != "added").then(|| ContentSide {
                path: previous.into(),
                source: AnchorSource::Commit {
                    sha: base_commit.clone(),
                    content_hash: AnchorSource::hash(old.as_bytes()),
                },
                content: old,
            });
            let new = (file.status != "removed").then(|| ContentSide {
                path: file.path.clone(),
                source: AnchorSource::Commit {
                    sha: head_commit.clone(),
                    content_hash: AnchorSource::hash(loaded.content.as_bytes()),
                },
                content: loaded.content,
            });
            (old, new)
        } else {
            use crate::database::types::SourceKind;
            let root = self.get_local_root(diff).await?;
            let (base, new_kind, head) = match &diff.comparison {
                ReviewComparison::Directory => (None, SourceKind::LocalFile, None),
                ReviewComparison::Working { base_commit } => {
                    (Some(base_commit.clone()), SourceKind::WorkingFile, None)
                }
                ReviewComparison::Staged { base_commit } => {
                    (Some(base_commit.clone()), SourceKind::IndexFile, None)
                }
                ReviewComparison::Branch {
                    base_commit,
                    head_commit,
                } => (
                    Some(base_commit.clone()),
                    SourceKind::Commit,
                    Some(head_commit.clone()),
                ),
                _ => return Err(invalid("Comparison does not belong to its target")),
            };
            let read = |path: String, kind, commit: Option<String>| {
                let root = root.clone();
                async move {
                    let selected_path = path.clone();
                    let (source, bytes) = tokio::task::spawn_blocking(move || {
                        super::workspace::get_source(&root, &selected_path, kind, commit.as_deref())
                    })
                    .await
                    .map_err(|_| invalid("Selected source read failed"))??;
                    Ok::<_, Error>(ContentSide {
                        path,
                        source,
                        content: String::from_utf8(bytes)
                            .map_err(|_| invalid("Selected file is not UTF-8 text"))?,
                    })
                }
            };
            let old = if file.status != "added" {
                if let Some(base) = base {
                    Some(read(previous.into(), SourceKind::Commit, Some(base)).await?)
                } else {
                    None
                }
            } else {
                None
            };
            let new = if !["deleted", "removed"].contains(&file.status.as_str()) {
                Some(read(file.path.clone(), new_kind, head).await?)
            } else {
                None
            };
            if let Some(ContentSide {
                source: AnchorSource::IndexFile { blob_oid, .. },
                ..
            }) = &new
                && file.identity.split(':').nth(1) != Some(blob_oid.as_str())
            {
                return Err(Error::Conflict(
                    "Index changed during the selected-file read".into(),
                ));
            }
            (old, new)
        };
        // Recheck metadata after the bounded source reads, including both PR sides and index identities.
        self.get_files(diff).await?;
        Ok(DiffContent { file, old, new })
    }

    // Private -----------------------------------------------------------------

    // Content loading and identity --------------------------------------------
    async fn get_provider(&self) -> Result<GithubClient> {
        match &self.provider {
            Some(provider) => Ok(provider.clone()),
            None => GithubClient::from_environment().await,
        }
    }

    async fn get_local_root(&self, diff: &DiffComparison) -> Result<PathBuf> {
        let root = match &diff.target {
            ReviewTarget::Directory { path } if diff.comparison == ReviewComparison::Directory => {
                Path::new(path).canonicalize()?
            }
            ReviewTarget::LocalGit { path, branch }
                if matches!(
                    diff.comparison,
                    ReviewComparison::Working { .. }
                        | ReviewComparison::Staged { .. }
                        | ReviewComparison::Branch { .. }
                ) =>
            {
                let (root, _, current_branch) = super::workspace::get_git_location(path).await?;
                if branch.as_ref().is_some_and(|b| b != &current_branch) {
                    return Err(Error::Conflict("Selected branch changed".into()));
                }
                PathBuf::from(root)
            }
            _ => return Err(invalid("Local source does not belong to this comparison")),
        };
        if diff.root.as_deref() != root.to_str() {
            return Err(invalid("Comparison root differs from its explicit target"));
        }
        Ok(root)
    }

    async fn get_metadata(&self, diff: &DiffComparison) -> Result<Vec<DiffFile>> {
        diff.comparison.validate()?;
        if let (
            ReviewTarget::PullRequest {
                repository_url,
                number,
            },
            ReviewComparison::PullRequest {
                base_commit,
                head_commit,
            },
        ) = (&diff.target, &diff.comparison)
        {
            if diff.root.is_some() {
                return Err(invalid(
                    "Remote comparison cannot claim a local content root",
                ));
            }
            let repo = Repository::from_repository_url(repository_url)?;
            let (_, files) = self
                .get_remote_workspace(&repo, *number, Some(base_commit), head_commit, "", true)
                .await?;
            return Ok(files
                .into_iter()
                .map(|(file, _)| DiffFile {
                    path: file.path,
                    previous_path: file.previous_path,
                    status: file.status,
                    identity: file.content_hash,
                })
                .collect());
        }
        let root = self.get_local_root(diff).await?;
        if diff.comparison == ReviewComparison::Directory {
            return tokio::task::spawn_blocking(move || {
                let mut files = Vec::new();
                Self::get_directory_metadata(&root, &root, &mut files)?;
                files.sort_by(|a, b| a.path.cmp(&b.path));
                Ok(files)
            })
            .await
            .map_err(|_| invalid("Directory metadata read failed"))?;
        }
        let comparison = diff.comparison.clone();
        tokio::task::spawn_blocking(move || {
            use gix::object::tree::diff::{Action, ChangeDetached};
            let repo = gix::open(&root)?.with_object_memory();
            let base = match &comparison {
                ReviewComparison::Working { base_commit }
                | ReviewComparison::Staged { base_commit }
                | ReviewComparison::Branch { base_commit, .. } => base_commit,
                _ => return Err(invalid("Unsupported local comparison")),
            };
            let id = gix::ObjectId::from_hex(base.as_bytes())
                .map_err(|_| invalid("Expected a full commit identity"))?;
            let old = repo.find_commit(id)?.tree()?;
            let new = match &comparison {
                ReviewComparison::Branch { head_commit, .. } => repo
                    .find_commit(
                        gix::ObjectId::from_hex(head_commit.as_bytes())
                            .map_err(|_| invalid("Expected a full commit identity"))?,
                    )?
                    .tree()?,
                ReviewComparison::Staged { .. } => {
                    repo.find_tree(git::write_index_tree(&repo, &repo)?)?
                }
                ReviewComparison::Working { .. } => repo.find_tree(git::write_files(
                    &repo,
                    &root,
                    git::get_worktree_paths(&repo)?,
                )?)?,
                _ => return Err(invalid("Unsupported local comparison")),
            };
            let mut changes = Vec::new();
            old.changes()?.for_each_to_obtain_tree(&new, |change| {
                changes.push(change.detach());
                Ok(Action::Continue(()))
            })?;
            let null = gix::ObjectId::null(repo.object_hash());
            let mut files = Vec::new();
            for change in changes {
                let (path, previous, status, old_id, new_id, mode) = match change {
                    ChangeDetached::Addition {
                        location,
                        id,
                        entry_mode,
                        ..
                    } => (location, Default::default(), "added", null, id, entry_mode),
                    ChangeDetached::Deletion {
                        location,
                        id,
                        entry_mode,
                        ..
                    } => (
                        location,
                        Default::default(),
                        "deleted",
                        id,
                        null,
                        entry_mode,
                    ),
                    ChangeDetached::Modification {
                        location,
                        previous_id,
                        id,
                        entry_mode,
                        previous_entry_mode,
                        ..
                    } => (
                        location,
                        Default::default(),
                        if entry_mode.kind() != previous_entry_mode.kind() {
                            "type_changed"
                        } else {
                            "modified"
                        },
                        previous_id,
                        id,
                        entry_mode,
                    ),
                    ChangeDetached::Rewrite {
                        source_location,
                        source_id,
                        location,
                        id,
                        copy,
                        entry_mode,
                        ..
                    } => (
                        location,
                        source_location,
                        if copy { "copied" } else { "renamed" },
                        source_id,
                        id,
                        entry_mode,
                    ),
                };
                if mode.is_tree() {
                    continue;
                }
                let path = path
                    .to_str()
                    .map_err(|_| invalid("Git path is not UTF-8"))?
                    .to_owned();
                Anchor::File { path: path.clone() }.validate()?;
                let stamp = if matches!(comparison, ReviewComparison::Working { .. }) {
                    Self::get_file_stamp(&root.join(&path))?
                } else {
                    String::new()
                };
                files.push(DiffFile {
                    path,
                    previous_path: previous
                        .to_str()
                        .map_err(|_| invalid("Git path is not UTF-8"))?
                        .to_owned(),
                    status: status.into(),
                    identity: format!("{old_id}:{new_id}:{stamp}"),
                });
                if files.len() > 5000 {
                    return Err(invalid("Diff exceeds 5000 files"));
                }
            }
            files.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(files)
        })
        .await
        .map_err(|_| invalid("Git comparison metadata read failed"))?
    }

    fn get_observation_token(diff: &DiffComparison, files: &[DiffFile]) -> Result<String> {
        Ok(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &diff.target,
                &diff.comparison,
                files
            ))?)
        ))
    }

    fn get_file_stamp(path: &Path) -> Result<String> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(format!(
                "{}:{:?}:{:?}",
                metadata.len(),
                metadata.modified()?,
                metadata.file_type()
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("missing".into()),
            Err(e) => Err(e.into()),
        }
    }

    fn get_directory_metadata(path: &Path, root: &Path, files: &mut Vec<DiffFile>) -> Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Ok(());
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_name() != ".git" {
                    Self::get_directory_metadata(&entry.path(), root, files)?;
                }
            }
        } else if metadata.is_file() {
            if files.len() >= 5000 {
                return Err(invalid(
                    "Directory exceeds 5000 files; select a narrower root",
                ));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid("Content escapes its explicit root"))?
                .to_str()
                .ok_or_else(|| invalid("Path is not UTF-8"))?
                .to_owned();
            files.push(DiffFile {
                path: relative,
                previous_path: String::new(),
                status: if metadata.len() > 2 * 1024 * 1024 {
                    "too_large"
                } else {
                    "present"
                }
                .into(),
                identity: Self::get_file_stamp(path)?,
            });
        }
        Ok(())
    }
}
