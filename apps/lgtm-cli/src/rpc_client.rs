//! Generated review client; command modules do not own transport details.

use std::time::Duration;

use anyhow::Result;
use connectrpc::{
    Protocol,
    client::{CallOptions, ClientConfig, HttpClient},
};

use crate::{
    backend::proto::lgtm::v1::{
        AddRequest, AddResponse, CloseReviewRequest, CloseReviewResponse, CommitRequest,
        CommitResponse, CreateCommentRequest, CreateCommentResponse, CreateReviewRequest,
        CreateReviewResponse, DeleteCommentRequest, DeleteCommentResponse, ListReviewsRequest,
        ListReviewsResponse, PushRequest, PushResponse, RefreshReviewRequest,
        RefreshReviewResponse, ReviewServiceClient, ShowReviewRequest, ShowReviewResponse,
        UpdateCommentRequest, UpdateCommentResponse,
    },
    daemon,
    store::Store,
};

const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Connect RPC client for `lgtm.v1.ReviewService`.
pub struct Client {
    endpoint_url: String,
}

impl Client {
    /// Connect to `LGTM_RPC_URL` if set, or reuse/auto-start the background daemon from `store`.
    pub async fn connect(store: &Store) -> Result<Self> {
        if let Ok(url) = std::env::var("LGTM_RPC_URL")
            && !url.trim().is_empty()
        {
            return Ok(Self {
                endpoint_url: url.trim().to_owned(),
            });
        }
        let instance = daemon::ensure_running(0, store).await?;
        Ok(Self {
            endpoint_url: format!("{}/rpc", instance.url()),
        })
    }

    // Public ------------------------------------------------------------------

    // Git & Snapshots

    /// Stage worktree paths into LGTM snapshot storage (`local == true`) or the working checkout (`local == false`).
    pub async fn add(&self, request: AddRequest) -> Result<AddResponse> {
        let response = self
            .service_client()?
            .add_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Commit staged or modified changes (`local == true` shadow commit or `local == false` working tree commit).
    pub async fn commit(&self, request: CommitRequest) -> Result<CommitResponse> {
        let response = self
            .service_client()?
            .commit_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Push committed changes on the active branch to the remote repository (`local == false` only).
    pub async fn push(&self, request: PushRequest) -> Result<PushResponse> {
        let response = self
            .service_client()?
            .push_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    // Reviews

    /// List reviews in the repository filtered by `local` and `all`.
    pub async fn get_reviews(&self, request: ListReviewsRequest) -> Result<ListReviewsResponse> {
        let response = self
            .service_client()?
            .list_reviews_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Create a local snapshot review (`local == true`) or synchronize a remote pull request review (`local == false`).
    pub async fn create_review(
        &self,
        request: CreateReviewRequest,
    ) -> Result<CreateReviewResponse> {
        let response = self
            .service_client()?
            .create_review_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Refresh an existing review's commits, comments, and line anchors.
    pub async fn refresh_review(
        &self,
        request: RefreshReviewRequest,
    ) -> Result<RefreshReviewResponse> {
        let response = self
            .service_client()?
            .refresh_review_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Fetch a review with its sessions, visible comments, and inline diff hunks.
    pub async fn get_review_detail(
        &self,
        request: ShowReviewRequest,
    ) -> Result<ShowReviewResponse> {
        let response = self
            .service_client()?
            .show_review_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Close/publish the caller's draft session with an outcome and optional summary body.
    pub async fn close_review(&self, request: CloseReviewRequest) -> Result<CloseReviewResponse> {
        let response = self
            .service_client()?
            .close_review_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    // Comments

    /// Create a draft inline, range, general, or reply comment.
    pub async fn create_comment(
        &self,
        request: CreateCommentRequest,
    ) -> Result<CreateCommentResponse> {
        let response = self
            .service_client()?
            .create_comment_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Update an existing comment's body, anchor, or thread resolution state.
    pub async fn update_comment(
        &self,
        request: UpdateCommentRequest,
    ) -> Result<UpdateCommentResponse> {
        let response = self
            .service_client()?
            .update_comment_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    /// Delete a comment by its sequential number.
    pub async fn delete_comment(
        &self,
        request: DeleteCommentRequest,
    ) -> Result<DeleteCommentResponse> {
        let response = self
            .service_client()?
            .delete_comment_with_options(request, Self::call_options())
            .await?;
        Ok(response.into_owned())
    }

    // Private -----------------------------------------------------------------

    fn service_client(&self) -> Result<ReviewServiceClient<HttpClient>> {
        let config = ClientConfig::new(self.endpoint_url.parse()?).with_protocol(Protocol::Connect);
        Ok(ReviewServiceClient::new(HttpClient::plaintext(), config))
    }

    fn call_options() -> CallOptions {
        CallOptions::default().with_timeout(RPC_TIMEOUT)
    }
}
