//! Connect RPC transport handlers for `lgtm.v1.ReviewService`.

use std::sync::Arc;

use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};

use crate::{
    database::Database,
    forge::RemoteForge,
    models::{ReviewGroupBy, ReviewOutcome},
    proto::lgtm::v1::{
        AddRequest, AddResponse, CloseReviewRequest, CloseReviewResponse, CommentDetail,
        CommitRequest, CommitResponse, CreateCommentRequest, CreateCommentResponse,
        CreateReviewRequest, CreateReviewResponse, DeleteCommentRequest, DeleteCommentResponse,
        DiffHunkDetail, HelloReviewRequest, HelloReviewResponse, IdentitySummary,
        ListReviewsRequest, ListReviewsResponse, PushRequest, PushResponse, RefreshReviewRequest,
        RefreshReviewResponse, ReviewOutcome as ProtoReviewOutcome, ReviewService, ReviewSummary,
        SessionSummary, SetFileReviewedRequest, SetFileReviewedResponse, ShowReviewRequest,
        ShowReviewResponse, UpdateCommentRequest, UpdateCommentResponse,
    },
    services::{CreateCommentInput, ReviewService as DomainReviewService, UpdateCommentInput},
    snapshots::SnapshotStore,
};

/// Connect RPC service adapter delegating to `services::ReviewService`.
pub struct ReviewApi {
    reviews: DomainReviewService,
}

impl ReviewApi {
    /// Construct a `ReviewApi` backed by `database` and default snapshot/forge adapters.
    pub fn new(database: Database) -> Self {
        Self {
            reviews: DomainReviewService::new(database),
        }
    }

    /// Construct a `ReviewApi` with explicit `SnapshotStore` and `RemoteForge` dependencies.
    pub fn with_dependencies(
        database: Database,
        snapshots: SnapshotStore,
        forge: Arc<dyn RemoteForge>,
    ) -> Self {
        Self {
            reviews: DomainReviewService::with_dependencies(database, snapshots, forge),
        }
    }
}

impl ReviewService for ReviewApi {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/Add"))]
    async fn add<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, AddRequest>,
    ) -> ServiceResult<impl Encodable<AddResponse> + Send + use<'a>> {
        let paths: Vec<String> = request.paths.iter().map(|s| (*s).to_owned()).collect();
        let staged_paths = self
            .reviews
            .add_paths(request.git_dir, &paths, request.local)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(AddResponse {
            staged_paths,
            local: request.local,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/Commit"))]
    async fn commit<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, CommitRequest>,
    ) -> ServiceResult<impl Encodable<CommitResponse> + Send + use<'a>> {
        let outcome = self
            .reviews
            .commit_changes(
                request.git_dir,
                request.message,
                request.as_agent,
                request.local,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(CommitResponse {
            commit_sha: outcome.commit_sha,
            branch: outcome.branch,
            author: Some(IdentitySummary::from(outcome.author)).into(),
            local: outcome.local,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/Push"))]
    async fn push<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, PushRequest>,
    ) -> ServiceResult<impl Encodable<PushResponse> + Send + use<'a>> {
        let outcome = self
            .reviews
            .push_changes(request.git_dir, request.local)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(PushResponse {
            branch: outcome.branch,
            head_sha: outcome.head_sha,
            repository_url: outcome.repository_url,
            local: outcome.local,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/ListReviews"))]
    async fn list_reviews<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, ListReviewsRequest>,
    ) -> ServiceResult<impl Encodable<ListReviewsResponse> + Send + use<'a>> {
        let reviews = self
            .reviews
            .get_reviews(request.git_dir, request.local, request.all)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(ListReviewsResponse {
            reviews: reviews.into_iter().map(ReviewSummary::from).collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/CreateReview"))]
    async fn create_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, CreateReviewRequest>,
    ) -> ServiceResult<impl Encodable<CreateReviewResponse> + Send + use<'a>> {
        let (review, session) = self
            .reviews
            .create_review(request.git_dir, request.local)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(CreateReviewResponse {
            review: Some(ReviewSummary::from(review)).into(),
            session: Some(SessionSummary::from(session)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/RefreshReview"))]
    async fn refresh_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, RefreshReviewRequest>,
    ) -> ServiceResult<impl Encodable<RefreshReviewResponse> + Send + use<'a>> {
        let (review, updated_anchors) = self
            .reviews
            .refresh_review(request.git_dir, request.selector, request.local)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(RefreshReviewResponse {
            review: Some(ReviewSummary::from(review)).into(),
            updated_anchors,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/ShowReview"))]
    async fn show_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, ShowReviewRequest>,
    ) -> ServiceResult<impl Encodable<ShowReviewResponse> + Send + use<'a>> {
        let group_by = request
            .group_by
            .as_known()
            .map(ReviewGroupBy::from)
            .unwrap_or_default();
        let detail = self
            .reviews
            .get_review_detail(
                request.git_dir,
                request.selector,
                request.local,
                group_by,
                request.unresolved,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(ShowReviewResponse {
            review: Some(ReviewSummary::from(detail.review)).into(),
            sessions: detail
                .sessions
                .into_iter()
                .map(SessionSummary::from)
                .collect(),
            comments: detail
                .comments
                .into_iter()
                .map(CommentDetail::from)
                .collect(),
            diff_hunks: detail
                .diff_hunks
                .into_iter()
                .map(DiffHunkDetail::from)
                .collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/CloseReview"))]
    async fn close_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, CloseReviewRequest>,
    ) -> ServiceResult<impl Encodable<CloseReviewResponse> + Send + use<'a>> {
        let proto_outcome = request
            .with_outcome
            .as_known()
            .unwrap_or(ProtoReviewOutcome::REVIEW_OUTCOME_COMMENT);
        let outcome = ReviewOutcome::try_from(proto_outcome).map_err(ConnectError::from)?;
        let (review, session) = self
            .reviews
            .close_review(
                request.git_dir,
                request.selector,
                request.local,
                outcome,
                request.body.map(str::to_owned),
                request.as_agent,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(CloseReviewResponse {
            review: Some(ReviewSummary::from(review)).into(),
            session: Some(SessionSummary::from(session)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/CreateComment"))]
    async fn create_comment<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, CreateCommentRequest>,
    ) -> ServiceResult<impl Encodable<CreateCommentResponse> + Send + use<'a>> {
        let comment = self
            .reviews
            .create_comment(CreateCommentInput {
                git_dir: request.git_dir.to_owned(),
                review_selector: request.review_selector.map(str::to_owned),
                body: request.body.to_owned(),
                file_path: request.file_path.map(str::to_owned),
                start_line: request.start_line,
                line: request.line,
                reply_to: request.reply_to,
                local: request.local,
                as_agent: request.as_agent.map(str::to_owned),
            })
            .await
            .map_err(ConnectError::from)?;
        Response::ok(CreateCommentResponse {
            comment: Some(CommentDetail::from(comment)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/UpdateComment"))]
    async fn update_comment<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, UpdateCommentRequest>,
    ) -> ServiceResult<impl Encodable<UpdateCommentResponse> + Send + use<'a>> {
        let comment = self
            .reviews
            .update_comment(UpdateCommentInput {
                git_dir: request.git_dir.to_owned(),
                comment_number: request.comment_number,
                body: request.body.map(str::to_owned),
                file_path: request.file_path.map(str::to_owned),
                start_line: request.start_line,
                line: request.line,
                resolved: request.resolved,
                local: request.local,
                as_agent: request.as_agent.map(str::to_owned),
            })
            .await
            .map_err(ConnectError::from)?;
        Response::ok(UpdateCommentResponse {
            comment: Some(CommentDetail::from(comment)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/DeleteComment"))]
    async fn delete_comment<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, DeleteCommentRequest>,
    ) -> ServiceResult<impl Encodable<DeleteCommentResponse> + Send + use<'a>> {
        let deleted_number = self
            .reviews
            .delete_comment(
                request.git_dir,
                request.comment_number,
                request.local,
                request.as_agent,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(DeleteCommentResponse {
            deleted_number,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/SetFileReviewed"))]
    async fn set_file_reviewed<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, SetFileReviewedRequest>,
    ) -> ServiceResult<impl Encodable<SetFileReviewedResponse> + Send + use<'a>> {
        let outcome = self
            .reviews
            .set_file_reviewed(
                request.git_dir,
                request.review_selector,
                request.file_path,
                request.viewed,
                request.local,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(SetFileReviewedResponse {
            file_path: outcome.file_path,
            viewed: outcome.viewed,
            commit_sha: outcome.commit_sha,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/HelloReview"))]
    async fn hello_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, HelloReviewRequest>,
    ) -> ServiceResult<impl Encodable<HelloReviewResponse> + Send + use<'a>> {
        Response::ok(HelloReviewResponse {
            message: self.reviews.get_hello(request.name),
            ..Default::default()
        })
    }
}
