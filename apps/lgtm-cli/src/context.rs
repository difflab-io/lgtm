//! Read-only Git/path discovery. Context selection never changes cwd or repository state.
use crate::backend::{
    models::{Actor, CommitContext, ContextKey},
    services::AuthService,
};
use anyhow::{Context, Result, bail};
use clap::Args;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Args, Clone, Debug, Default)]
pub struct ContextArgs {
    #[arg(long,conflicts_with_all=["branch","notes"])]
    pub pr: Option<String>,
    #[arg(long, conflicts_with = "notes")]
    pub branch: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value=".")]
    pub notes: Option<PathBuf>,
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub repo: Option<String>,
    /// Forge hostname for --pr and shorthand repository names.
    #[arg(long, default_value = "github.com")]
    pub host: String,
}
#[derive(Args, Clone, Debug, Default)]
pub struct ActorArgs {
    #[arg(long = "as", conflicts_with = "identity")]
    pub as_name: Option<String>,
    #[arg(long, requires = "as_name")]
    pub model: Option<String>,
    #[arg(long)]
    pub identity: Option<String>,
}
pub struct Selected {
    pub key: ContextKey,
    pub path: PathBuf,
    pub commits: CommitContext,
    pub repository: Option<String>,
}
pub trait GitContextProvider {
    fn output(&self, path: &Path, args: &[&str]) -> Option<String>;
}
pub struct LocalGit;
impl GitContextProvider for LocalGit {
    fn output(&self, path: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }
}
pub fn repository_name(raw: &str, host: &str) -> Result<String> {
    let raw = raw.trim().trim_end_matches('/').trim_end_matches(".git");
    if raw.starts_with("local:") {
        return Ok(raw.into());
    }
    let remote = if let Some((_, rest)) = raw.split_once("://") {
        rest.split_once('@')
            .map(|(_, tail)| tail)
            .unwrap_or(rest)
            .to_string()
    } else if let Some((userhost, path)) = raw.split_once(':') {
        format!(
            "{}/{}",
            userhost.rsplit('@').next().unwrap_or(userhost),
            path
        )
    } else if raw.split('/').count() == 2 {
        format!("{host}/{raw}")
    } else {
        raw.into()
    };
    if remote.split('/').count() != 3 || remote.split('/').any(str::is_empty) {
        bail!("Repository must be HOST/OWNER/REPO, OWNER/REPO, or a Git remote URL");
    }
    Ok(remote.to_lowercase())
}
impl ContextArgs {
    pub fn select(&self, git: &impl GitContextProvider) -> Result<Selected> {
        let path = self
            .notes
            .as_ref()
            .or(self.path.as_ref())
            .cloned()
            .unwrap_or(std::env::current_dir()?)
            .canonicalize()
            .context("Review path must exist and be accessible")?;
        let git_path = if path.is_file() {
            path.parent().unwrap_or(&path)
        } else {
            &path
        };
        let in_git = git
            .output(git_path, &["rev-parse", "--show-toplevel"])
            .is_some();
        let discovered = if in_git {
            if let Some(remote) = git.output(git_path, &["remote", "get-url", "origin"]) {
                Some(repository_name(&remote, &self.host)?)
            } else {
                git.output(
                    git_path,
                    &["rev-parse", "--path-format=absolute", "--git-common-dir"],
                )
                .map(|p| {
                    format!(
                        "local:{}",
                        PathBuf::from(p)
                            .canonicalize()
                            .unwrap_or_else(|_| git_path.into())
                            .display()
                    )
                })
            }
        } else {
            None
        };
        let repository = self
            .repo
            .as_deref()
            .map(|r| repository_name(r, &self.host))
            .transpose()?
            .or(discovered.clone());
        let commits = CommitContext {
            head: git.output(git_path, &["rev-parse", "HEAD"]),
            base: git
                .output(git_path, &["merge-base", "HEAD", "origin/main"])
                .or_else(|| git.output(git_path, &["rev-parse", "HEAD^"])),
        };
        let key = if let Some(pr) = &self.pr {
            let (repo, number) = pr
                .rsplit_once('#')
                .context("PR must be OWNER/REPO#NUMBER")?;
            let number: i64 = number.parse().context("PR number must be positive")?;
            let repo = repository_name(repo, &self.host)?;
            if self.repo.as_ref().is_some() && repository.as_deref() != Some(&repo) {
                bail!("--repo conflicts with the explicit PR repository");
            }
            ContextKey::PullRequest {
                host: repo.split('/').next().unwrap().into(),
                repository: repo,
                number,
            }
        } else if self.notes.is_some() || (!in_git && self.branch.is_none() && repository.is_none())
        {
            ContextKey::Notes {
                path: path.to_str().context("Path is not UTF-8")?.into(),
            }
        } else {
            let repo = repository.context("Branch context needs a repository; specify --repo")?;
            let branch=self.branch.clone().or_else(||git.output(git_path,&["symbolic-ref","--quiet","HEAD"])).context("Detached HEAD or no current branch; choose --pr, --notes or an explicit --branch/--repo")?;
            let branch = if branch.starts_with("refs/heads/") {
                branch
            } else {
                format!("refs/heads/{branch}")
            };
            if branch.contains(['\n', '\r', ' '])
                || branch.contains("..")
                || branch.contains("@{")
                || branch.ends_with('/')
                || branch.ends_with(".lock")
            {
                bail!("Invalid branch name");
            }
            ContextKey::Branch {
                repository: repo,
                branch,
            }
        };
        key.validate()?;
        let repository = key.repository().map(str::to_string);
        // An explicitly selected foreign branch must not inherit the checkout's commits.
        let commits = if let ContextKey::Branch { branch, repository } = &key {
            if discovered.as_deref() != Some(repository.as_str()) {
                CommitContext::default()
            } else if self.branch.is_some() {
                CommitContext {
                    head: git.output(git_path, &["rev-parse", branch]),
                    base: git.output(git_path, &["merge-base", branch, "origin/main"]),
                }
            } else {
                commits
            }
        } else if matches!(key, ContextKey::PullRequest { .. }) {
            CommitContext::default()
        } else {
            commits
        };
        Ok(Selected {
            key,
            path,
            commits,
            repository,
        })
    }
}
impl ActorArgs {
    pub async fn resolve(
        &self,
        auth: &AuthService,
        path: &Path,
        git: &impl GitContextProvider,
    ) -> Result<Actor> {
        if let Some(id) = &self.identity {
            return Ok(auth.identity(id).await?);
        }
        let (name, email) = if let Some(value) = &self.as_name {
            let (name, email) = value
                .rsplit_once('<')
                .context("--as must be 'Name <email>'")?;
            (
                name.trim().to_string(),
                email
                    .strip_suffix('>')
                    .context("--as must end with >")?
                    .trim()
                    .to_string(),
            )
        } else {
            let base = if path.is_file() {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            (
                git.output(base, &["config", "user.name"])
                    .context("Missing Git identity; use --as 'Name <email>'")?,
                git.output(base, &["config", "user.email"])
                    .context("Missing Git email; use --as 'Name <email>'")?,
            )
        };
        Ok(auth
            .ensure_identity(&name, &email, self.model.clone(), None)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct GitFixture;
    impl GitContextProvider for GitFixture {
        fn output(&self, _: &Path, args: &[&str]) -> Option<String> {
            Some(
                match args {
                    ["rev-parse", "--show-toplevel"] => "/checkout",
                    ["remote", "get-url", "origin"] => "git@github.com:acme/local.git",
                    ["symbolic-ref", "--quiet", "HEAD"] => "refs/heads/main",
                    ["rev-parse", "HEAD"] => "local-head",
                    ["merge-base", "HEAD", "origin/main"] => "local-base",
                    _ => return None,
                }
                .into(),
            )
        }
    }
    #[test]
    fn explicit_foreign_repository_does_not_inherit_local_commits() {
        let dir = tempfile::tempdir().unwrap();
        let selected = ContextArgs {
            path: Some(dir.path().into()),
            repo: Some("acme/foreign".into()),
            host: "github.com".into(),
            ..Default::default()
        }
        .select(&GitFixture)
        .unwrap();
        assert_eq!(
            selected.repository.as_deref(),
            Some("github.com/acme/foreign")
        );
        assert!(selected.commits.head.is_none() && selected.commits.base.is_none());
    }
}
