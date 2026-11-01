//! GitHub implementation of `RemoteForge` backed by `extensions::octocrab`.

use crate::{
    error::{Error, Result},
    extensions::octocrab,
    forge::{
        RemoteForge,
        models::{
            ForgeCommentUpdate, ForgeDraftCommentInput, ForgeDraftCommentReceipt,
            ForgePullRequestBundle, ForgeReviewReceipt,
        },
    },
    models::ReviewOutcome,
    utils::parse_github_owner_repo,
};

/// Remote forge provider for GitHub pull requests, reviews, comments, and viewed-file states.
#[derive(Clone, Debug, Default)]
pub struct GitHubForge {
    token: Option<String>,
    api_base_url: Option<String>,
}

impl GitHubForge {
    /// Construct a `GitHubForge` using ambient environment credentials (`GITHUB_TOKEN` or `GH_TOKEN`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a `GitHubForge` with an explicit token and optional API base URL.
    pub fn with_credentials(token: Option<String>, api_base_url: Option<String>) -> Self {
        Self {
            token,
            api_base_url,
        }
    }

    // Private -----------------------------------------------------------------

    fn client(&self) -> Result<::octocrab::Octocrab> {
        octocrab::build_client(self.token.as_deref(), self.api_base_url.as_deref())
    }
}

#[async_trait::async_trait]
impl RemoteForge for GitHubForge {
    async fn push_branch(
        &self,
        repository_url: &str,
        branch: &str,
        payload: &crate::extensions::gitoxide::HeadCommitPayload,
    ) -> Result<Option<String>> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        let sha = octocrab::push_commit_to_branch(&client, &owner, &repo, branch, payload).await?;
        Ok(Some(sha))
    }

    async fn get_pull_request_bundle(
        &self,
        repository_url: &str,
        branch_or_pr: &str,
    ) -> Result<ForgePullRequestBundle> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::fetch_pull_request_bundle(&client, &owner, &repo, branch_or_pr).await
    }

    async fn create_pending_review(
        &self,
        repository_url: &str,
        pr_number: u64,
        head_sha: &str,
    ) -> Result<String> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::create_pending_review(&client, &owner, &repo, pr_number, head_sha).await
    }

    async fn create_remote_draft_comment(
        &self,
        repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        comment: &ForgeDraftCommentInput,
    ) -> Result<ForgeDraftCommentReceipt> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::create_draft_comment(
            &client,
            &owner,
            &repo,
            pr_number,
            pending_review_id,
            comment,
        )
        .await
    }

    async fn update_remote_comment(
        &self,
        repository_url: &str,
        forge_id: &str,
        update: &ForgeCommentUpdate,
    ) -> Result<()> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::update_comment(&client, &owner, &repo, forge_id, update).await
    }

    async fn delete_remote_comment(&self, repository_url: &str, forge_id: &str) -> Result<()> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::delete_comment(&client, &owner, &repo, forge_id).await
    }

    async fn set_remote_file_viewed(
        &self,
        _repository_url: &str,
        pr_forge_id: Option<&str>,
        _pr_number: u64,
        file_path: &str,
        viewed: bool,
    ) -> Result<()> {
        let pr_node_id = pr_forge_id.ok_or_else(|| {
            Error::Invalid(
                "ForgeBacked review is missing pr_forge_id for viewed-file sync".to_owned(),
            )
        })?;
        let client = self.client()?;
        octocrab::set_file_viewed(&client, pr_node_id, file_path, viewed).await
    }

    async fn submit_pending_review(
        &self,
        repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        outcome: ReviewOutcome,
        body: Option<&str>,
    ) -> Result<ForgeReviewReceipt> {
        let (owner, repo) = parse_github_owner_repo(repository_url)?;
        let client = self.client()?;
        octocrab::submit_review(
            &client,
            &owner,
            &repo,
            pr_number,
            pending_review_id,
            outcome,
            body,
        )
        .await
    }
}
