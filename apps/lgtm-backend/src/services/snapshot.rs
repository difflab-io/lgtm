//! Store review source as ordinary Git commits in LGTM's own repository.
use crate::{
    error::{Result, invalid},
    extensions::git,
    models::{ReviewComparison, ReviewTarget, Snapshot},
};
use gix::{
    bstr::{BString, ByteSlice},
    refs::transaction::{PreviousValue, RefEdit},
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Snapshot objects and refs are independent of the user's checkout, branch and index.
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

    pub async fn capture(
        &self,
        target: ReviewTarget,
        comparison: ReviewComparison,
    ) -> Result<Snapshot> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.capture_saved(target, comparison))
            .await
            .map_err(|_| invalid("Snapshot capture task failed"))?
    }

    /// Remove only unpublished capture refs; retained revisions are never passed here.
    pub async fn discard(&self, snapshot: &Snapshot) -> Result<()> {
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || {
            let repo = gix::open(&snapshot.root)?;
            let mut edits = Vec::new();
            for side in ["base", "head"] {
                let name = format!("refs/lgtm/revisions/{}/{side}", snapshot.id);
                if let Some(reference) = repo.try_find_reference(name.as_str())? {
                    edits.push(RefEdit::delete(
                        reference.name().to_owned(),
                        PreviousValue::Any,
                    ));
                }
            }
            repo.edit_references(edits)?;
            Ok(())
        })
        .await
        .map_err(|_| invalid("Snapshot cleanup task failed"))?
    }

    // Private -----------------------------------------------------------------

    fn capture_saved(
        &self,
        target: ReviewTarget,
        comparison: ReviewComparison,
    ) -> Result<Snapshot> {
        comparison.validate()?;
        let source = match &target {
            ReviewTarget::LocalGit { path, .. } | ReviewTarget::Directory { path } => {
                Some(Path::new(path).canonicalize()?)
            }
            _ => None,
        };
        let source_repo = if matches!(target, ReviewTarget::LocalGit { .. }) {
            Some(gix::discover(
                source
                    .as_ref()
                    .ok_or_else(|| invalid("Source directory unavailable"))?,
            )?)
        } else {
            None
        };
        let address = match &target {
            ReviewTarget::PullRequest { repository_url, .. } => repository_url.clone(),
            ReviewTarget::LocalGit { .. } => {
                let repo = source_repo
                    .as_ref()
                    .ok_or_else(|| invalid("Source Git repository unavailable"))?;
                repo.config_snapshot()
                    .string("remote.origin.url")
                    .map(|url| {
                        url.to_str()
                            .map(str::to_owned)
                            .map_err(|_| invalid("Git origin is not UTF-8"))
                    })
                    .transpose()?
                    .unwrap_or_else(|| repo.common_dir().to_string_lossy().into_owned())
            }
            ReviewTarget::Directory { .. } => source
                .as_ref()
                .ok_or_else(|| invalid("Source directory unavailable"))?
                .to_string_lossy()
                .into_owned(),
        };
        let digest = Sha256::digest(Self::get_repository_address(&address).as_bytes());
        let repository_id = Uuid::from_bytes(
            digest[..16]
                .try_into()
                .map_err(|_| invalid("Invalid repository identity"))?,
        );
        let root = self.root.join(repository_id.to_string());
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let options = gix::open::Options::default().config_overrides([
            "credential.helper=",
            "credential.helper=!gh auth git-credential",
            "credential.interactive=false",
        ]);
        let repo = if root.join(".git").exists() {
            gix::open_opts(&root, options)?
        } else {
            gix::ThreadSafeRepository::init_opts(
                &root,
                gix::create::Kind::WithWorktree,
                gix::create::Options {
                    object_hash: source_repo.as_ref().map(gix::Repository::object_hash),
                    ..Default::default()
                },
                options,
            )?
            .to_thread_local()
        };
        let base_commit = match &comparison {
            ReviewComparison::Working { base_commit }
            | ReviewComparison::Staged { base_commit }
            | ReviewComparison::Branch { base_commit, .. }
            | ReviewComparison::PullRequest { base_commit, .. } => Some(base_commit),
            ReviewComparison::Directory => None,
        };
        let base = if let Some(commit) = base_commit {
            let remote = match &target {
                ReviewTarget::PullRequest { repository_url, .. } => {
                    Self::get_remote(repository_url)
                }
                _ => source
                    .as_ref()
                    .ok_or_else(|| invalid("Source directory unavailable"))?
                    .to_string_lossy()
                    .into_owned(),
            };
            let mut commits = vec![commit.to_owned()];
            if let ReviewComparison::Branch { head_commit, .. }
            | ReviewComparison::PullRequest { head_commit, .. } = &comparison
            {
                commits.push(head_commit.clone());
            } else if let Some(source) = &source_repo {
                commits.push(source.head_id()?.to_string());
            }
            git::fetch_commits(&repo, &remote, &commits)?;
            gix::ObjectId::from_hex(commit.as_bytes())
                .map_err(|_| invalid("Expected a full commit identity"))?
        } else {
            Self::commit(&repo, repo.empty_tree().id, None)?
        };
        let head = match &comparison {
            ReviewComparison::Branch { head_commit, .. }
            | ReviewComparison::PullRequest { head_commit, .. } => {
                gix::ObjectId::from_hex(head_commit.as_bytes())
                    .map_err(|_| invalid("Expected a full commit identity"))?
            }
            _ => {
                let source = source
                    .as_ref()
                    .ok_or_else(|| invalid("Source directory unavailable"))?;
                let parent = match &source_repo {
                    Some(source) => source.head_id()?.detach(),
                    None => base,
                };
                let tree = if matches!(comparison, ReviewComparison::Staged { .. }) {
                    git::write_index_tree(
                        &repo,
                        source_repo
                            .as_ref()
                            .ok_or_else(|| invalid("Staged snapshot requires Git"))?,
                    )?
                } else {
                    let paths = if let Some(source_repo) = &source_repo {
                        git::get_worktree_paths(source_repo)?.into_iter().collect()
                    } else {
                        let mut paths = Vec::new();
                        Self::collect_paths(source, source, &mut paths)?;
                        paths
                    };
                    let store = self.root.canonicalize()?;
                    let mut selected = Vec::new();
                    for path in paths {
                        if !source
                            .join(gix::path::from_bstr(path.as_bstr())?)
                            .starts_with(&store)
                        {
                            selected.push(path);
                        }
                    }
                    git::write_files(&repo, source, selected)?
                };
                if repo.find_commit(parent)?.tree_id()?.detach() == tree {
                    parent
                } else {
                    Self::commit(&repo, tree, Some(parent))?
                }
            }
        };
        let id = Uuid::new_v4();
        repo.edit_references([
            RefEdit::update(
                format!("refs/lgtm/revisions/{id}/base")
                    .try_into()
                    .map_err(|_| invalid("Invalid snapshot ref"))?,
                base,
                PreviousValue::MustNotExist,
                "LGTM snapshot",
            ),
            RefEdit::update(
                format!("refs/lgtm/revisions/{id}/head")
                    .try_into()
                    .map_err(|_| invalid("Invalid snapshot ref"))?,
                head,
                PreviousValue::MustNotExist,
                "LGTM snapshot",
            ),
        ])?;
        Ok(Snapshot {
            id,
            repository_id,
            root: root.to_string_lossy().into_owned(),
            base_commit: base.to_string(),
            head_commit: head.to_string(),
        })
    }

    fn commit(
        repo: &gix::Repository,
        tree: gix::ObjectId,
        parent: Option<gix::ObjectId>,
    ) -> Result<gix::ObjectId> {
        let signature = gix::actor::Signature {
            name: "LGTM".into(),
            email: "snapshots@lgtm.local".into(),
            time: gix::date::Time {
                seconds: 946684800,
                offset: 0,
            },
        };
        Ok(repo
            .write_object(&gix::objs::Commit {
                tree,
                parents: parent.into_iter().collect(),
                author: signature.clone(),
                committer: signature,
                encoding: None,
                message: "LGTM review snapshot\n".into(),
                extra_headers: Vec::new(),
            })?
            .detach())
    }

    fn collect_paths(path: &Path, root: &Path, files: &mut Vec<BString>) -> Result<()> {
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
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("Source path escapes snapshot root"))?;
                files.push(gix::path::into_bstr(relative)?.into_owned());
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
        if std::env::var("LGTM_GITHUB_TEST_MODE").as_deref() == Ok("1")
            && let Ok(remote) = std::env::var("LGTM_GITHUB_TEST_GIT_URL")
        {
            return remote;
        }
        repository_url.to_owned()
    }
}
