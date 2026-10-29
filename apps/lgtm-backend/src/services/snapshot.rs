//! Capture saved source in an owned Git store without changing the user's index or refs.
use crate::{
    error::{Result, invalid},
    extensions::git,
    models::{ReviewComparison, ReviewTarget, Snapshot},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Stores have their own object database and index. Revision refs retain every captured commit.
#[derive(Clone)]
pub struct SnapshotService {
    root: PathBuf,
}

impl SnapshotService {
    // Public ------------------------------------------------------------------

    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn from_environment() -> Self {
        let home = std::env::var_os("LGTM_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".lgtm")
            });
        Self::new(home.join("snapshots"))
    }

    /// Capture only saved bytes. Existing branch/PR commits retain their original identities.
    pub async fn capture(
        &self,
        target: ReviewTarget,
        comparison: ReviewComparison,
    ) -> Result<Snapshot> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.capture_saved(&target, &comparison))
            .await
            .map_err(|_| invalid("Snapshot capture task failed"))?
    }

    /// Release refs for a capture that was not published. Existing saved revisions are never discarded here.
    pub async fn discard(&self, snapshot: &Snapshot) -> Result<()> {
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || {
            for side in ["base", "head"] {
                let reference = format!("refs/lgtm/revisions/{}/{side}", snapshot.id);
                git::run(
                    Path::new(&snapshot.root),
                    &["update-ref", "-d", &reference],
                    &[],
                    None,
                )?;
            }
            Ok(())
        })
        .await
        .map_err(|_| invalid("Snapshot cleanup task failed"))?
    }

    // Private -----------------------------------------------------------------

    fn capture_saved(
        &self,
        target: &ReviewTarget,
        comparison: &ReviewComparison,
    ) -> Result<Snapshot> {
        comparison.validate()?;
        let source = match target {
            ReviewTarget::LocalGit { path, .. } | ReviewTarget::Directory { path } => {
                Some(Path::new(path).canonicalize()?)
            }
            _ => None,
        };
        let address = match target {
            ReviewTarget::PullRequest { repository_url, .. } => repository_url
                .trim_end_matches('/')
                .trim_end_matches(".git")
                .to_owned(),
            ReviewTarget::LocalGit { .. } => {
                let source = source
                    .as_ref()
                    .ok_or_else(|| invalid("Source directory unavailable"))?;
                git::get_bytes(source, &["config", "--get", "remote.origin.url"])
                    .map(|v| {
                        String::from_utf8_lossy(&v)
                            .trim()
                            .trim_end_matches('/')
                            .trim_end_matches(".git")
                            .to_owned()
                    })
                    .or_else(|_| {
                        git::get_bytes(
                            source,
                            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
                        )
                        .map(|v| String::from_utf8_lossy(&v).trim().to_owned())
                    })?
            }
            ReviewTarget::Directory { .. } => source
                .as_ref()
                .ok_or_else(|| invalid("Source directory unavailable"))?
                .to_string_lossy()
                .into_owned(),
        };
        let address = Self::get_repository_address(&address);
        let digest = Sha256::digest(address.as_bytes());
        let repository_id = Uuid::from_bytes(
            digest[..16]
                .try_into()
                .map_err(|_| invalid("Invalid repository identity"))?,
        );
        let root = self.root.join(repository_id.to_string());
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        // Git's own lock files serialize refs. Each capture has a separate index.
        if !root.join(".git").exists() {
            let format = source
                .as_ref()
                .and_then(|source| {
                    git::get_bytes(source, &["rev-parse", "--show-object-format"]).ok()
                })
                .map(Self::text)
                .transpose()?
                .unwrap_or_else(|| "sha1".into());
            git::run(
                &root,
                &["init", "--quiet", &format!("--object-format={format}")],
                &[],
                None,
            )?;
        }
        let id = Uuid::new_v4();
        let index = root.join(format!(".capture-{id}.index"));
        let result = self.capture_commits(&root, source.as_deref(), target, comparison, &index);
        let _ = std::fs::remove_file(&index);
        let (base_commit, head_commit) = result?;
        let references = format!(
            "start\nupdate refs/lgtm/revisions/{id}/base {base_commit}\nupdate refs/lgtm/revisions/{id}/head {head_commit}\nprepare\ncommit\n"
        );
        git::run(
            &root,
            &["update-ref", "--stdin"],
            &[],
            Some(references.as_bytes()),
        )?;
        Ok(Snapshot {
            id,
            repository_id,
            root: root.to_string_lossy().into_owned(),
            base_commit,
            head_commit,
        })
    }

    fn capture_commits(
        &self,
        root: &Path,
        source: Option<&Path>,
        target: &ReviewTarget,
        comparison: &ReviewComparison,
        index: &Path,
    ) -> Result<(String, String)> {
        let env = vec![("GIT_INDEX_FILE", index.as_os_str())];
        let base = match comparison {
            ReviewComparison::Directory => {
                git::run(root, &["read-tree", "--empty"], &env, None)?;
                let tree = Self::text(git::run(root, &["write-tree"], &env, None)?)?;
                Self::commit(root, &tree, None)?
            }
            ReviewComparison::Working { base_commit }
            | ReviewComparison::Staged { base_commit }
            | ReviewComparison::Branch { base_commit, .. }
            | ReviewComparison::PullRequest { base_commit, .. } => {
                let remote = match target {
                    ReviewTarget::PullRequest { repository_url, .. } => {
                        Self::get_remote(repository_url)
                    }
                    _ => source
                        .and_then(Path::to_str)
                        .ok_or_else(|| invalid("Source Git directory unavailable"))?
                        .to_owned(),
                };
                git::run(
                    root,
                    &[
                        "-c",
                        "credential.helper=!gh auth git-credential",
                        "fetch",
                        "--quiet",
                        "--no-tags",
                        "--no-write-fetch-head",
                        &remote,
                        base_commit,
                    ],
                    &[],
                    None,
                )?;
                base_commit.clone()
            }
        };
        if let ReviewComparison::Branch { head_commit, .. }
        | ReviewComparison::PullRequest { head_commit, .. } = comparison
        {
            let remote = match target {
                ReviewTarget::PullRequest { repository_url, .. } => {
                    Self::get_remote(repository_url)
                }
                _ => source
                    .and_then(Path::to_str)
                    .ok_or_else(|| invalid("Source Git directory unavailable"))?
                    .to_owned(),
            };
            git::run(
                root,
                &[
                    "-c",
                    "credential.helper=!gh auth git-credential",
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--no-write-fetch-head",
                    &remote,
                    head_commit,
                ],
                &[],
                None,
            )?;
            return Ok((base, head_commit.clone()));
        }
        let source = source.ok_or_else(|| invalid("Source directory unavailable"))?;
        let parent = if matches!(comparison, ReviewComparison::Directory) {
            base.clone()
        } else {
            let head = Self::text(git::get_bytes(source, &["rev-parse", "HEAD^{commit}"])?)?;
            git::run(
                root,
                &[
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--no-write-fetch-head",
                    source
                        .to_str()
                        .ok_or_else(|| invalid("Source path must be UTF-8"))?,
                    &head,
                ],
                &[],
                None,
            )?;
            git::run(root, &["read-tree", &head], &env, None)?;
            head
        };
        if matches!(comparison, ReviewComparison::Staged { .. }) {
            let patch = git::get_bytes(
                source,
                &["diff", "--cached", "--binary", "--full-index", "HEAD", "--"],
            )?;
            if !patch.is_empty() {
                git::run(
                    root,
                    &["apply", "--cached", "--binary", "--whitespace=nowarn", "-"],
                    &env,
                    Some(&patch),
                )?;
            }
        } else {
            let paths = if matches!(comparison, ReviewComparison::Directory) {
                let mut files = Vec::new();
                Self::collect_paths(source, source, &mut files)?;
                files
            } else {
                let paths = git::get_bytes(
                    source,
                    &[
                        "ls-files",
                        "--cached",
                        "--others",
                        "--exclude-standard",
                        "-z",
                    ],
                )?;
                paths
                    .split(|byte| *byte == 0)
                    .filter(|path| !path.is_empty())
                    .map(|path| {
                        String::from_utf8(path.to_vec())
                            .map_err(|_| invalid("Snapshot paths must be UTF-8"))
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let store = self.root.canonicalize()?;
            let paths: Vec<_> = paths
                .into_iter()
                .filter(|path| !source.join(path).starts_with(&store))
                .collect();
            Self::capture_files(root, source, &paths, &env)?;
        }
        let tree = Self::text(git::run(root, &["write-tree"], &env, None)?)?;
        let original_tree = Self::text(git::get_bytes(
            root,
            &["rev-parse", &format!("{parent}^{{tree}}")],
        )?)?;
        let head = if tree == original_tree {
            parent
        } else {
            Self::commit(root, &tree, Some(&parent))?
        };
        Ok((base, head))
    }

    /// Write literal bytes, bypassing clean filters and line-ending conversion.
    fn capture_files(
        root: &Path,
        source: &Path,
        paths: &[String],
        env: &[(&str, &OsStr)],
    ) -> Result<()> {
        use std::collections::BTreeSet;
        let format = Self::text(git::get_bytes(
            root,
            &["rev-parse", "--show-object-format"],
        )?)?;
        let null_oid = "0".repeat(if format == "sha256" { 64 } else { 40 });
        let mut entries = Vec::new();
        for path in paths.iter().collect::<BTreeSet<_>>() {
            let selected = source.join(path);
            let metadata = match std::fs::symlink_metadata(&selected) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    entries.extend_from_slice(format!("0 {null_oid}\t{path}\0").as_bytes());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let (mode, bytes) = if metadata.file_type().is_symlink() {
                let target = std::fs::read_link(selected)?;
                ("120000", target.as_os_str().as_encoded_bytes().to_vec())
            } else if metadata.is_file() {
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                (
                    if executable { "100755" } else { "100644" },
                    std::fs::read(&selected)?,
                )
            } else {
                if metadata.is_dir() {
                    let head =
                        Self::text(git::get_bytes(&selected, &["rev-parse", "HEAD^{commit}"])?)?;
                    entries.extend_from_slice(format!("160000 {head}\t{path}\0").as_bytes());
                }
                continue;
            };
            let oid = Self::text(git::run(
                root,
                &["hash-object", "-w", "--no-filters", "--stdin"],
                &[],
                Some(&bytes),
            )?)?;
            entries.extend_from_slice(format!("{mode} {oid}\t{path}\0").as_bytes());
        }
        if !entries.is_empty() {
            git::run(
                root,
                &["update-index", "-z", "--index-info"],
                env,
                Some(&entries),
            )?;
        }
        Ok(())
    }

    fn collect_paths(path: &Path, root: &Path, files: &mut Vec<String>) -> Result<()> {
        if files.len() >= 5000 {
            return Err(invalid(
                "Directory exceeds 5000 files; select a narrower directory",
            ));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                Self::collect_paths(&entry.path(), root, files)?;
            } else if kind.is_file() {
                if files.len() >= 5000 {
                    return Err(invalid(
                        "Directory exceeds 5000 files; select a narrower directory",
                    ));
                }
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| invalid("Source path escapes snapshot root"))?
                    .to_string_lossy()
                    .into_owned();
                files.push(relative);
            }
        }
        Ok(())
    }

    fn get_repository_address(address: &str) -> String {
        let address = address
            .trim()
            .trim_end_matches('/')
            .trim_end_matches(".git");
        if let Some(remote) = address.strip_prefix("git@")
            && let Some((host, path)) = remote.split_once(':')
        {
            return format!("https://{host}/{path}");
        }
        if let Ok(url) = url::Url::parse(address)
            && ["https", "http", "ssh"].contains(&url.scheme())
            && let Some(host) = url.host_str()
        {
            return format!("https://{host}{}", url.path());
        }
        address.to_owned()
    }

    fn get_remote(repository_url: &str) -> String {
        // The recorded forge transport supplies its owned Git bundle alongside its API endpoint.
        if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() == Ok("1")
            && let Ok(remote) = std::env::var("LGTM_GITHUB_TEST_GIT_URL")
        {
            return remote;
        }
        repository_url.to_owned()
    }

    fn commit(root: &Path, tree: &str, parent: Option<&str>) -> Result<String> {
        let env: Vec<(&str, &OsStr)> = vec![
            ("GIT_AUTHOR_DATE", OsStr::new("2000-01-01T00:00:00Z")),
            ("GIT_COMMITTER_DATE", OsStr::new("2000-01-01T00:00:00Z")),
            ("GIT_AUTHOR_NAME", OsStr::new("LGTM")),
            ("GIT_AUTHOR_EMAIL", OsStr::new("snapshots@lgtm.local")),
            ("GIT_COMMITTER_NAME", OsStr::new("LGTM")),
            ("GIT_COMMITTER_EMAIL", OsStr::new("snapshots@lgtm.local")),
        ];
        let mut args = vec!["commit-tree", tree];
        if let Some(parent) = parent {
            args.extend(["-p", parent]);
        }
        args.extend(["-m", "LGTM review snapshot"]);
        Self::text(git::run(root, &args, &env, None)?)
    }

    fn text(bytes: Vec<u8>) -> Result<String> {
        String::from_utf8(bytes)
            .map(|s| s.trim().to_owned())
            .map_err(|_| invalid("Git returned a non-UTF-8 identity"))
    }
}
