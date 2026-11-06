//! Capture policy for local review and note sources. Git runs only against separate storage.
use crate::{
    error::{Result, invalid},
    extensions::git,
    models::*,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

type SnapshotFile = (String, String, Vec<u8>);

struct SnapshotProject {
    root: PathBuf,
    slug: String,
    is_git: bool,
}

/// Backend-owned storage; defaults to ~/.lgtm/local or LGTM_HOME/local.
#[derive(Clone, Default)]
pub struct SnapshotService {
    local_root: Option<PathBuf>,
}

impl SnapshotService {
    pub fn new(local_root: PathBuf) -> Self {
        Self {
            local_root: Some(local_root),
        }
    }

    // Public ------------------------------------------------------------------

    /// Capture the validated file bytes, not a later reread of the working file.
    pub async fn capture_file(
        &self,
        root: PathBuf,
        path: String,
        bytes: Vec<u8>,
        source: AnchorSource,
        id: Uuid,
    ) -> Result<LocalSnapshot> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let root = root.canonicalize()?;
            let project = Self::get_project(&root)?;
            let absolute = root.join(&path);
            let path = absolute
                .strip_prefix(&project.root)
                .map_err(|_| invalid("Snapshot path escapes project"))?
                .to_str()
                .ok_or_else(|| invalid("Snapshot path requires UTF-8"))?
                .to_owned();
            let parent = match source {
                AnchorSource::Commit { sha, .. } => Some(sha),
                _ => None,
            };
            let commit_oid = service.persist(
                &project,
                &[(path.clone(), "100644".into(), bytes)],
                parent.as_deref(),
                &format!("refs/lgtm/threads/{id}"),
                false,
            )?;
            Ok(LocalSnapshot {
                project_slug: project.slug,
                commit_oid,
                path,
            })
        })
        .await
        .map_err(|_| invalid("Snapshot capture task failed"))?
    }

    /// A new review retains its full selected Git tree. Notes capture only on writes.
    pub async fn capture_review(&self, context: &Context, session: &Session) -> Result<()> {
        let ContextKey::Local {
            location: LocalLocation::Branch { .. },
        } = &context.key
        else {
            return Ok(());
        };
        let Some(root) = context.paths.first().map(PathBuf::from) else {
            return Ok(());
        };
        let session = session.clone();
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let project = Self::get_project(&root)?;
            if !project.is_git {
                return Err(invalid("Local Git review requires its checkout"));
            }
            let files = Self::get_review_files(&project.root, &session)?;
            service.persist(
                &project,
                &files,
                session.commits.head.as_deref(),
                &format!("refs/lgtm/reviews/{}", session.id),
                true,
            )?;
            Ok(())
        })
        .await
        .map_err(|_| invalid("Review snapshot task failed"))?
    }

    /// Read retained source after edits, commits, restarts, or source deletion.
    pub async fn get_content(
        &self,
        snapshot: &LocalSnapshot,
        expected_hash: &str,
    ) -> Result<String> {
        snapshot.validate()?;
        let service = self.clone();
        let snapshot = snapshot.clone();
        let expected_hash = expected_hash.to_owned();
        tokio::task::spawn_blocking(move || {
            let git_dir = service.get_local_root()?.join(&snapshot.project_slug);
            let mut command = git::command(&git_dir);
            command.args([
                "cat-file",
                "blob",
                &format!("{}:{}", snapshot.commit_oid, snapshot.path),
            ]);
            let bytes = git::run(command, None)?;
            if AnchorSource::hash(&bytes) != expected_hash {
                return Err(invalid("Retained source failed content verification"));
            }
            String::from_utf8(bytes).map_err(|_| invalid("Retained source requires UTF-8"))
        })
        .await
        .map_err(|_| invalid("Snapshot read task failed"))?
    }

    // Private -----------------------------------------------------------------

    fn get_local_root(&self) -> Result<PathBuf> {
        if let Some(root) = &self.local_root {
            return Ok(root.clone());
        }
        let home = std::env::var_os("LGTM_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".lgtm")))
            .ok_or_else(|| invalid("Set LGTM_HOME for local snapshot storage"))?;
        Ok(home.join("local"))
    }

    fn get_project(root: &Path) -> Result<SnapshotProject> {
        let root = root.canonicalize()?;
        let mut command = git::command(&root);
        command.args([
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ]);
        let result = command.output()?;
        let (project_root, identity, is_git) = if result.status.success() {
            let output = String::from_utf8(result.stdout)
                .map_err(|_| invalid("Project path requires UTF-8"))?;
            let mut paths = output.lines();
            let project_root = PathBuf::from(
                paths
                    .next()
                    .ok_or_else(|| invalid("Missing project root"))?,
            );
            let common = PathBuf::from(
                paths
                    .next()
                    .ok_or_else(|| invalid("Missing common Git directory"))?,
            )
            .canonicalize()?;
            (project_root, common, true)
        } else {
            (root.clone(), root, false)
        };
        let name_root = if is_git {
            identity.parent().unwrap_or(&identity)
        } else {
            &identity
        };
        let name: String = name_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .take(48)
            .collect();
        let hash = format!(
            "{:x}",
            Sha256::digest(identity.as_os_str().as_encoded_bytes())
        );
        Ok(SnapshotProject {
            root: project_root,
            slug: format!(
                "{}-{}",
                if name.is_empty() { "project" } else { &name },
                &hash[..12]
            ),
            is_git,
        })
    }

    fn get_review_files(root: &Path, session: &Session) -> Result<Vec<SnapshotFile>> {
        let mut command = git::command(root);
        let committed = session.comparison_kind == ComparisonKind::Branch;
        let staged = session.comparison_kind == ComparisonKind::Staged;
        if committed {
            command.args([
                "ls-tree",
                "-r",
                "-z",
                session
                    .commits
                    .head
                    .as_deref()
                    .ok_or_else(|| invalid("Branch review requires a head"))?,
            ]);
        } else if staged {
            command.args(["ls-files", "--stage", "-z"]);
        } else {
            command.args([
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ]);
        }
        let output = git::run(command, None)?;
        let mut files = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in output.split(|b| *b == 0).filter(|v| !v.is_empty()) {
            let entry =
                std::str::from_utf8(entry).map_err(|_| invalid("Snapshot paths require UTF-8"))?;
            let (path, mode, bytes) = if committed || staged {
                let (meta, path) = entry
                    .split_once('\t')
                    .ok_or_else(|| invalid("Invalid snapshot tree entry"))?;
                let meta: Vec<_> = meta.split_whitespace().collect();
                if meta.len() != 3
                    || staged && meta[2] != "0"
                    || !["100644", "100755", "120000"].contains(&meta[0])
                {
                    return Err(invalid(
                        "Snapshot requires resolved files; submodules are outside this PoC",
                    ));
                }
                let oid = if committed { meta[2] } else { meta[1] };
                let mut command = git::command(root);
                command.args(["cat-file", "blob", oid]);
                (
                    path.to_owned(),
                    meta[0].to_owned(),
                    git::run(command, None)?,
                )
            } else {
                let path = root.join(entry);
                // A tracked directory can have been replaced by a working symlink.
                for parent in path
                    .ancestors()
                    .skip(1)
                    .take_while(|parent| *parent != root)
                {
                    match fs::symlink_metadata(parent) {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            return Err(invalid("Working snapshot crosses a nested symlink"));
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                        Err(error) => return Err(error.into()),
                    }
                }
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let (mode, bytes) = if metadata.file_type().is_symlink() {
                    (
                        "120000",
                        fs::read_link(&path)?
                            .as_os_str()
                            .as_encoded_bytes()
                            .to_vec(),
                    )
                } else {
                    if !metadata.is_file() {
                        return Err(invalid(
                            "Snapshot does not support submodules or special files",
                        ));
                    }
                    #[cfg(unix)]
                    let executable = {
                        use std::os::unix::fs::PermissionsExt;
                        metadata.permissions().mode() & 0o111 != 0
                    };
                    #[cfg(not(unix))]
                    let executable = false;
                    (
                        if executable { "100755" } else { "100644" },
                        fs::read(path)?,
                    )
                };
                (entry.to_owned(), mode.to_owned(), bytes)
            };
            if seen.insert(path.clone()) {
                files.push((path, mode, bytes));
            }
        }
        Ok(files)
    }

    fn persist(
        &self,
        project: &SnapshotProject,
        files: &[SnapshotFile],
        parent: Option<&str>,
        reference: &str,
        complete_tree: bool,
    ) -> Result<String> {
        let git_dir = self.get_local_root()?.join(&project.slug);
        fs::create_dir_all(&git_dir)?;
        let mut command = git::command(&git_dir);
        command.args(["init", "--bare", "--object-format=sha1", "--template="]);
        git::run(command, None)?;
        let parent = if project.is_git {
            let mut command = git::command(&project.root);
            command.args(["rev-parse", "--verify", parent.unwrap_or("HEAD")]);
            let output = command.output()?;
            if !output.status.success() {
                if parent.is_some() {
                    return Err(invalid("Snapshot parent commit is unavailable"));
                }
                None // Notes and working files can exist before the first developer commit.
            } else {
                let oid = String::from_utf8(output.stdout)
                    .map_err(|_| invalid("Invalid Git identity"))?
                    .trim()
                    .to_owned();
                let mut command = git::command(&git_dir);
                command
                    .args(["fetch", "--no-tags", "--no-write-fetch-head", "--"])
                    .arg(&project.root)
                    .arg(format!("{oid}:refs/lgtm/commits/{oid}"));
                git::run(command, None)?;
                Some(oid)
            }
        } else {
            None
        };
        let index = tempfile::tempdir()?;
        let index_path = index.path().join("index");
        let mut command = git::command(&git_dir);
        command.env("GIT_INDEX_FILE", &index_path).arg("read-tree");
        command.arg(if complete_tree {
            "--empty"
        } else {
            parent.as_deref().unwrap_or("--empty")
        });
        git::run(command, None)?;
        let mut entries = Vec::new();
        for (path, mode, bytes) in files {
            Anchor::File { path: path.clone() }.validate()?;
            let mut command = git::command(&git_dir);
            command.args(["hash-object", "-w", "--stdin", "--no-filters"]);
            let oid = String::from_utf8(git::run(command, Some(bytes))?)
                .map_err(|_| invalid("Invalid blob identity"))?;
            entries.extend_from_slice(format!("{mode} {}\t{path}\0", oid.trim()).as_bytes());
        }
        let mut command = git::command(&git_dir);
        command
            .env("GIT_INDEX_FILE", &index_path)
            .args(["update-index", "-z", "--index-info"]);
        git::run(command, Some(&entries))?;
        let mut command = git::command(&git_dir);
        command.env("GIT_INDEX_FILE", &index_path).arg("write-tree");
        let tree = String::from_utf8(git::run(command, None)?)
            .map_err(|_| invalid("Invalid tree identity"))?;
        let mut command = git::command(&git_dir);
        command
            .env("GIT_AUTHOR_NAME", "LGTM")
            .env("GIT_AUTHOR_EMAIL", "snapshot@lgtm.invalid")
            .env("GIT_COMMITTER_NAME", "LGTM")
            .env("GIT_COMMITTER_EMAIL", "snapshot@lgtm.invalid")
            .args(["commit-tree", tree.trim(), "-m", reference]);
        if let Some(parent) = &parent {
            command.args(["-p", parent]);
        }
        let oid = String::from_utf8(git::run(command, None)?)
            .map_err(|_| invalid("Invalid snapshot identity"))?
            .trim()
            .to_owned();
        let mut command = git::command(&git_dir);
        command.args(["update-ref", reference, &oid, &"0".repeat(40)]);
        git::run(command, None)?;
        tracing::info!(project_slug = %project.slug, git_dir = %git_dir.display(), commit_oid = %oid, reference, "Retained local source snapshot");
        Ok(oid)
    }
}
