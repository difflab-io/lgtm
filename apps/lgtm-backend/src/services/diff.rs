//! Content comparisons and selected-file reads. This service has no review database handle.
use crate::{
    error::{Error, Result, invalid},
    forge::{Github, Repository},
    models::{
        Anchor, AnchorSource, ContentSide, DiffComparison, DiffContent, DiffFile, ReviewComparison,
        ReviewTarget,
    },
    utils::WorkspaceFile,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// At most 32 immutable, selected remote blobs (1 MiB each), scoped to repository/path/commit.
/// This is process-local validation evidence, not review persistence or a directory content cache.
struct CachedSource {
    repository_url: String,
    path: String,
    commit: String,
    bytes: Arc<[u8]>,
}

#[derive(Clone, Default)]
pub struct DiffService {
    provider: Option<Github>,
    selected_sources: Arc<Mutex<VecDeque<CachedSource>>>,
}

impl DiffService {
    async fn get_provider(&self) -> Result<Github> {
        match &self.provider {
            Some(provider) => Ok(provider.clone()),
            None => Github::from_environment().await,
        }
    }

    /// Verify supplied PR evidence against immutable bytes, preferring associated local Git objects.
    /// Previously served immutable bytes can be verified without another forge request.
    pub async fn get_anchor_evidence(
        &self,
        paths: &[String],
        target: ReviewTarget,
        comparison: ReviewComparison,
        anchor: &Anchor,
        supplied: &crate::models::AnchorEvidence,
    ) -> Result<crate::models::AnchorEvidence> {
        use crate::models::AnchorEvidence;
        let (
            ReviewTarget::PullRequest {
                repository_url,
                number,
            },
            ReviewComparison::PullRequest {
                base_commit,
                head_commit,
            },
        ) = (target, comparison)
        else {
            return Err(invalid("PR source validation requires a PR comparison"));
        };
        let (path, range, old) = match anchor {
            Anchor::Line {
                path,
                start,
                end,
                side,
                ..
            } => (path, Some((*start, *end)), side == "old"),
            Anchor::File { path } => (
                path,
                None,
                matches!(&supplied.source, AnchorSource::Commit {sha,..} if sha == &base_commit && sha != &head_commit),
            ),
            Anchor::General => return Err(invalid("Review-level feedback has no file source")),
        };
        let sha = if old { &base_commit } else { &head_commit };
        let verify = |bytes: &[u8]| -> Result<AnchorEvidence> {
            let source = AnchorSource::Commit {
                sha: sha.clone(),
                content_hash: AnchorSource::hash(bytes),
            };
            let actual = if let Some((start, end)) = range {
                AnchorEvidence::capture(source, bytes, start, end)?
            } else {
                AnchorEvidence::capture_file(source, bytes)?
            };
            if actual != *supplied {
                return Err(Error::Conflict("Reviewed source or selected evidence changed; retain the comment body and refresh".into()));
            }
            Ok(actual)
        };
        for root in paths {
            let root = PathBuf::from(root);
            let path = path.clone();
            let sha = sha.clone();
            let source = tokio::task::spawn_blocking(move || {
                super::workspace::get_source(
                    &root,
                    &path,
                    crate::database::types::SourceKind::Commit,
                    Some(&sha),
                )
            })
            .await
            .map_err(|_| invalid("Local PR source read failed"))?;
            if let Ok((_, bytes)) = source {
                return verify(&bytes);
            }
        }
        let cached = self
            .selected_sources
            .lock()
            .map_err(|_| invalid("Selected-source cache is unavailable"))?
            .iter()
            .find(|source| {
                source.repository_url == repository_url.as_str()
                    && source.path == *path
                    && source.commit == *sha
            })
            .map(|source| source.bytes.clone());
        if let Some(bytes) = cached {
            return verify(&bytes);
        }
        let repo = Repository::parse(&repository_url)?;
        let (_, files) = self
            .get_remote_workspace(&repo, number, Some(&base_commit), &head_commit, "", true)
            .await?;
        let selected = files
            .iter()
            .find(|(file, _)| &file.path == path || (old && &file.previous_path == path))
            .map(|(file, _)| file.path.clone())
            .ok_or_else(|| invalid("Anchor file is outside this PR comparison"))?;
        let (_, files) = self
            .get_remote_workspace(
                &repo,
                number,
                Some(&base_commit),
                &head_commit,
                &selected,
                false,
            )
            .await?;
        let (file, old_content) = files
            .into_iter()
            .next()
            .ok_or_else(|| invalid("Selected PR source is unavailable"))?;
        if (old && file.status == "added") || (!old && file.status == "removed") {
            return Err(invalid("Selected PR side has no file"));
        }
        verify(if old {
            old_content.as_bytes()
        } else {
            file.content.as_bytes()
        })
    }

    /// Read PR metadata or one selected file at a pinned comparison, without review storage.
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
        let files = github
            .get_all(&format!("{}/pulls/{pr}/files", repo.route()))
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
        // Store only complete selected-file observations after both PR guards succeeded.
        if !metadata_only {
            let mut cache = self
                .selected_sources
                .lock()
                .map_err(|_| invalid("Selected-source cache is unavailable"))?;
            for (file, old) in &output {
                for (path, commit, bytes, absent) in [
                    (
                        if file.previous_path.is_empty() {
                            &file.path
                        } else {
                            &file.previous_path
                        },
                        &pull.base,
                        old.as_bytes(),
                        file.status == "added",
                    ),
                    (
                        &file.path,
                        &pull.head,
                        file.content.as_bytes(),
                        file.status == "removed",
                    ),
                ] {
                    if absent {
                        continue;
                    }
                    cache.retain(|source| {
                        !(source.repository_url == repo.repository_url
                            && source.path == *path
                            && source.commit == *commit)
                    });
                    cache.push_back(CachedSource {
                        repository_url: repo.repository_url.clone(),
                        path: path.clone(),
                        commit: commit.clone(),
                        bytes: Arc::from(bytes),
                    });
                    while cache.len() > 32 {
                        cache.pop_front();
                    }
                }
            }
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
                    None => {
                        let head = Self::get_git_bytes(
                            Path::new(&root),
                            &["rev-parse", "--verify", "HEAD^{commit}"],
                        )
                        .await?;
                        ReviewComparison::Working {
                            base_commit: String::from_utf8_lossy(&head).trim().into(),
                        }
                    }
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
                let repo = Repository::parse(&repository_url)?;
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
            let repo = Repository::parse(repository_url)?;
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
            let repo = Repository::parse(repository_url)?;
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
        let mut args = vec!["diff", "--no-ext-diff", "--raw", "--no-abbrev", "-z", "-M"];
        match &diff.comparison {
            ReviewComparison::Working { base_commit } => args.push(base_commit),
            ReviewComparison::Staged { base_commit } => {
                args.push("--cached");
                args.push(base_commit);
            }
            ReviewComparison::Branch {
                base_commit,
                head_commit,
            } => {
                args.push(base_commit);
                args.push(head_commit);
            }
            _ => return Err(invalid("Unsupported local comparison")),
        }
        args.push("--");
        let raw = Self::get_git_bytes(&root, &args).await?;
        let mut parts = raw.split(|b| *b == 0).filter(|part| !part.is_empty());
        let mut files = Vec::new();
        while let Some(header) = parts.next() {
            let header =
                std::str::from_utf8(header).map_err(|_| invalid("Git metadata is not UTF-8"))?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 5 || !fields[0].starts_with(':') {
                return Err(invalid("Incomplete Git file metadata"));
            }
            let first =
                std::str::from_utf8(parts.next().ok_or_else(|| invalid("Git path missing"))?)
                    .map_err(|_| invalid("Git path is not UTF-8"))?;
            let (path, previous_path) = if fields[4].starts_with('R') || fields[4].starts_with('C')
            {
                (
                    std::str::from_utf8(
                        parts
                            .next()
                            .ok_or_else(|| invalid("Git destination path missing"))?,
                    )
                    .map_err(|_| invalid("Git path is not UTF-8"))?,
                    first,
                )
            } else {
                (first, "")
            };
            Anchor::File { path: path.into() }.validate()?;
            let status = match fields[4].as_bytes()[0] {
                b'A' => "added",
                b'D' => "deleted",
                b'R' => "renamed",
                b'C' => "copied",
                b'T' => "type_changed",
                b'U' => "unmerged",
                _ => "modified",
            };
            let stamp = if matches!(diff.comparison, ReviewComparison::Working { .. }) {
                Self::get_file_stamp(&root.join(path))?
            } else {
                String::new()
            };
            files.push(DiffFile {
                path: path.into(),
                previous_path: previous_path.into(),
                status: status.into(),
                identity: format!("{}:{}:{stamp}", fields[2], fields[3]),
            });
            if files.len() > 5000 {
                return Err(invalid("Diff exceeds 5000 files"));
            }
        }
        if matches!(diff.comparison, ReviewComparison::Working { .. }) {
            let untracked =
                Self::get_git_bytes(&root, &["ls-files", "--others", "--exclude-standard", "-z"])
                    .await?;
            for path in untracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
                let path =
                    std::str::from_utf8(path).map_err(|_| invalid("Git path is not UTF-8"))?;
                files.push(DiffFile {
                    path: path.into(),
                    previous_path: String::new(),
                    status: "added".into(),
                    identity: Self::get_file_stamp(&root.join(path))?,
                });
                if files.len() > 5000 {
                    return Err(invalid("Diff exceeds 5000 files"));
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
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

    async fn get_git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::process::Command::new("git")
                .arg("--literal-pathspecs")
                .arg("-C")
                .arg(root)
                .args(args)
                .env("GIT_OPTIONAL_LOCKS", "0")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| invalid("Git content metadata timed out"))??;
        if !output.status.success() {
            return Err(invalid("Git comparison metadata is unavailable"));
        }
        if output.stdout.len() > 16 * 1024 * 1024 {
            return Err(invalid("Git metadata exceeds the bounded response limit"));
        }
        Ok(output.stdout)
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(provider: Github) -> Self {
        Self {
            provider: Some(provider),
            ..Self::default()
        }
    }
}
