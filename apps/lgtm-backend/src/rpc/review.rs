//! Review RPC handlers. Validation and representation conversions live with models.
use super::{Api, id, optional};
use crate::{models as m, proto::lgtm::v1::*};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};
impl ReviewService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_file_reviews"))]
    async fn get_file_reviews<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetFileReviewsRequest>,
    ) -> ServiceResult<impl Encodable<GetFileReviewsResponse> + Send + use<'a>> {
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let review_id = id(request.review_id)?;
        let selected = self
            .reviews
            .get_review(&review_id)
            .await
            .map_err(ConnectError::from)?;
        let marks = if matches!(selected.target, m::ReviewTarget::PullRequest { .. }) {
            self.forge.get_file_reviews(&review_id).await
        } else {
            self.reviews.get_file_reviews(&review_id, &who).await
        }
        .map_err(ConnectError::from)?;
        Response::ok(GetFileReviewsResponse {
            files: marks
                .into_iter()
                .map(|(path, content_identity)| FileReview {
                    path,
                    content_identity,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/set_file_reviewed"))]
    async fn set_file_reviewed<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SetFileReviewedRequest>,
    ) -> ServiceResult<impl Encodable<SetFileReviewedResponse> + Send + use<'a>> {
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let review_id = id(request.review_id)?;
        let selected = self
            .reviews
            .get_review(&review_id)
            .await
            .map_err(ConnectError::from)?;
        if matches!(selected.target, m::ReviewTarget::PullRequest { .. }) {
            self.forge
                .set_file_reviewed(&review_id, request.path, request.reviewed)
                .await
        } else {
            self.reviews
                .set_file_reviewed(
                    &review_id,
                    &who,
                    request.path,
                    request.reviewed.then_some(request.content_identity),
                )
                .await
        }
        .map_err(ConnectError::from)?;
        Response::ok(SetFileReviewedResponse::default())
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/start_review"))]
    async fn start_review<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, StartReviewRequest>,
    ) -> ServiceResult<impl Encodable<StartReviewResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let (replace_local, accept_pr_comparison) = match request.resume_policy.to_i32() {
            0 | 1 => (false, false),
            2 => (true, false),
            3 => (false, true),
            _ => return Err(ConnectError::invalid_argument("Unknown resume policy")),
        };
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let mut guard = m::MutationGuard::from_proto_fields(
            &request.expected_version,
            &request.request_key,
            replace_local || accept_pr_comparison,
        )?;
        let expected_active_id = optional(&request.expected_review_id)
            .map(|value| id(&value))
            .transpose()?;
        if (replace_local || accept_pr_comparison) && expected_active_id.is_none() {
            return Err(ConnectError::invalid_argument(
                "Select the active review before replacing it",
            ));
        }
        let target = m::ReviewTarget::from_proto(&request.target)?;
        if replace_local && matches!(target, m::ReviewTarget::PullRequest { .. }) {
            return Err(ConnectError::invalid_argument(
                "PR attempts require explicit submission or recovery",
            ));
        }
        if accept_pr_comparison && !matches!(target, m::ReviewTarget::PullRequest { .. }) {
            return Err(ConnectError::invalid_argument(
                "PR comparison acceptance requires a PR target",
            ));
        }
        let supplied = m::ReviewComparison::from_proto(&request.comparison)?;
        if matches!(target, m::ReviewTarget::LocalGit { .. })
            && !matches!(
                supplied,
                Some(
                    m::ReviewComparison::Working { .. }
                        | m::ReviewComparison::Staged { .. }
                        | m::ReviewComparison::Branch { .. }
                )
            )
        {
            return Err(ConnectError::invalid_argument(
                "Local Git review requires a working, staged, or branch comparison",
            ));
        }
        if matches!(target, m::ReviewTarget::Directory { .. })
            && supplied
                .as_ref()
                .is_some_and(|c| *c != m::ReviewComparison::Directory)
        {
            return Err(ConnectError::invalid_argument(
                "Directory target requires its directory comparison",
            ));
        }
        let mut context = self
            .reviews
            .resolve_target(&target)
            .await
            .map_err(ConnectError::from)?;
        if context.key.is_forge_backed() {
            let supplied_commits = supplied
                .as_ref()
                .map(|comparison| match comparison {
                    m::ReviewComparison::PullRequest {
                        base_commit,
                        head_commit,
                    } => Ok(m::CommitContext {
                        base: Some(base_commit.clone()),
                        head: Some(head_commit.clone()),
                    }),
                    _ => Err(ConnectError::invalid_argument(
                        "PR target requires a PR comparison",
                    )),
                })
                .transpose()?;
            if let Some(saved) = self
                .reviews
                .get_started_session(
                    &context,
                    &who,
                    supplied_commits,
                    &m::StartOptions {
                        comparison_kind: m::ComparisonKind::PullRequest,
                        replace_local,
                        accept_pr_comparison,
                        expected_active_id,
                    },
                    &guard,
                )
                .await
                .map_err(ConnectError::from)?
            {
                return Response::ok(StartReviewResponse {
                    review: Some(m::Review::into_proto(
                        self.reviews
                            .prepare_review(&saved.id)
                            .await
                            .map_err(ConnectError::from)?,
                    ))
                    .into(),
                    ..Default::default()
                });
            }
        }
        if !request.local_git_path.is_empty() {
            context = self
                .reviews
                .associate_checkout(&context.id, &request.local_git_path)
                .await
                .map_err(ConnectError::from)?;
        }
        let comparison = if context.key.is_forge_backed() {
            context = if accept_pr_comparison {
                self.forge
                    .refresh_for_acceptance(
                        &context.id,
                        &expected_active_id.ok_or_else(|| {
                            ConnectError::invalid_argument("Select the prior review")
                        })?,
                        &who,
                        guard.expected_version.ok_or_else(|| {
                            ConnectError::invalid_argument("Read the prior review version")
                        })?,
                    )
                    .await
                    .map_err(ConnectError::from)?
            } else if let Some(version) = guard.expected_version {
                self.forge
                    .refresh_at_version(&context.id, version)
                    .await
                    .map_err(ConnectError::from)?
            } else {
                self.forge
                    .refresh(&context.id)
                    .await
                    .map_err(ConnectError::from)?
            };
            guard = guard.after_observation(context.revision);
            if context.forge_state != Some(crate::database::types::ForgeState::Open) {
                return Err(ConnectError::failed_precondition(
                    "PR is closed or merged; saved reviews remain readable",
                ));
            }
            let authoritative = m::ReviewComparison::PullRequest {
                base_commit: context
                    .refreshed_base
                    .clone()
                    .ok_or_else(|| ConnectError::failed_precondition("PR base unavailable"))?,
                head_commit: context
                    .refreshed_head
                    .clone()
                    .ok_or_else(|| ConnectError::failed_precondition("PR head unavailable"))?,
            };
            if supplied.as_ref().is_some_and(|c| *c != authoritative) {
                return Err(ConnectError::failed_precondition("PR comparison changed"));
            }
            authoritative
        } else {
            supplied.unwrap_or(m::ReviewComparison::Directory)
        };
        let commits = match &comparison {
            m::ReviewComparison::Working { base_commit }
            | m::ReviewComparison::Staged { base_commit } => m::CommitContext {
                base: Some(base_commit.clone()),
                head: Some(base_commit.clone()),
            },
            m::ReviewComparison::Branch {
                base_commit,
                head_commit,
            }
            | m::ReviewComparison::PullRequest {
                base_commit,
                head_commit,
            } => m::CommitContext {
                base: Some(base_commit.clone()),
                head: Some(head_commit.clone()),
            },
            m::ReviewComparison::Directory => m::CommitContext::default(),
        };
        let saved = self
            .reviews
            .start_session(
                &context,
                &who,
                commits,
                m::StartOptions {
                    comparison_kind: comparison.kind(),
                    replace_local,
                    accept_pr_comparison,
                    expected_active_id,
                },
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(StartReviewResponse {
            review: Some(m::Review::into_proto(
                self.reviews
                    .prepare_review(&saved.id)
                    .await
                    .map_err(ConnectError::from)?,
            ))
            .into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_review"))]
    async fn get_review<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetReviewRequest>,
    ) -> ServiceResult<impl Encodable<GetReviewResponse> + Send + use<'a>> {
        Response::ok(GetReviewResponse {
            review: Some(m::Review::into_proto(
                self.reviews
                    .get_review(&id(request.review_id)?)
                    .await
                    .map_err(ConnectError::from)?,
            ))
            .into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/list_reviews"))]
    async fn list_reviews<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, ListReviewsRequest>,
    ) -> ServiceResult<impl Encodable<ListReviewsResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let target = request
            .target
            .as_option()
            .map(m::ReviewTarget::from_proto)
            .transpose()?;
        let after = optional(&request.page_token).map(|v| id(&v)).transpose()?;
        let size = if request.page_size == 0 {
            50
        } else {
            request.page_size.min(200)
        } as usize;
        let mut values = self
            .reviews
            .get_reviews(
                target.as_ref(),
                optional(&request.state).as_deref(),
                after,
                size as u64 + 1,
            )
            .await
            .map_err(ConnectError::from)?;
        let more = values.len() > size;
        values.truncate(size);
        let next = if more {
            values
                .last()
                .map(|r| r.session.id.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        Response::ok(ListReviewsResponse {
            reviews: values.into_iter().map(m::Review::into_proto).collect(),
            next_page_token: next,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/refresh_review"))]
    async fn refresh_review<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, RefreshReviewRequest>,
    ) -> ServiceResult<impl Encodable<RefreshReviewResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_fields(
            &request.expected_version,
            &request.request_key,
            true,
        )?;
        let m::ReviewScope::Review { review_id } = scope else {
            return Err(ConnectError::invalid_argument(
                "Live-buffer notes are no longer supported",
            ));
        };
        let selected = self
            .reviews
            .get_session(&review_id)
            .await
            .map_err(ConnectError::from)?;
        let context = self
            .reviews
            .get_context_by_id(&selected.context_id)
            .await
            .map_err(ConnectError::from)?;
        if selected.creator.id != who.id {
            return Err(ConnectError::permission_denied(
                "Only the session creator can refresh its revision",
            ));
        }
        if guard.expected_version != Some(context.revision) {
            return Err(ConnectError::failed_precondition(
                "Review changed; read it before refreshing",
            ));
        }
        if context.key.is_forge_backed() {
            self.forge
                .refresh(&selected.context_id)
                .await
                .map_err(ConnectError::from)?;
        }
        let mut review = self
            .reviews
            .get_review(&review_id)
            .await
            .map_err(ConnectError::from)?;
        if selected.state() == "active" {
            self.reviews
                .capture_snapshot(&review, true)
                .await
                .map_err(ConnectError::from)?;
        }
        review = self
            .reviews
            .get_review(&review_id)
            .await
            .map_err(ConnectError::from)?;
        let version = review.version.to_string();
        let refreshed = Some(review.into_proto());
        Response::ok(RefreshReviewResponse {
            review: refreshed.into(),
            version,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/complete_review"))]
    async fn complete_review<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, CompleteReviewRequest>,
    ) -> ServiceResult<impl Encodable<CompleteReviewResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let guard = m::MutationGuard::from_proto_fields(
            &request.expected_version,
            &request.request_key,
            true,
        )?;
        let selected = self
            .reviews
            .get_session(&id(&request.review_id)?)
            .await
            .map_err(ConnectError::from)?;
        if self
            .reviews
            .get_context_by_id(&selected.context_id)
            .await
            .map_err(ConnectError::from)?
            .key
            .is_forge_backed()
        {
            return Err(ConnectError::invalid_argument(
                "Use SubmitReview for a PR review",
            ));
        }
        let saved = self
            .reviews
            .complete(
                &selected.context_id,
                &who,
                m::Completion {
                    body: optional(&request.body),
                    ..Default::default()
                },
                &format!("complete:{}:{}", who.id, request.request_key),
                Some(&selected.id),
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(CompleteReviewResponse {
            review: Some(m::Review::into_proto(
                self.reviews
                    .get_review(&saved.id)
                    .await
                    .map_err(ConnectError::from)?,
            ))
            .into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_conversation"))]
    async fn get_conversation<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetConversationRequest>,
    ) -> ServiceResult<impl Encodable<GetConversationResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let snapshot = if request.snapshot_id.is_empty() {
            None
        } else {
            let m::ReviewScope::Review { review_id } = &scope else {
                return Err(ConnectError::invalid_argument(
                    "Saved revision requires a review",
                ));
            };
            Some(
                self.reviews
                    .get_snapshot(review_id, Some(&id(&request.snapshot_id)?))
                    .await
                    .map_err(ConnectError::from)?,
            )
        };
        let paths = (!request.file_paths.is_empty()).then_some(request.file_paths.as_slice());
        let (page, next) = self
            .reviews
            .get_conversation_page(
                &scope,
                paths,
                request.include_drafts.unwrap_or(true),
                &request.page_token,
                request.page_size,
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(match page {
            None => GetConversationResponse {
                version: "0".into(),
                ..Default::default()
            },
            Some(page) => GetConversationResponse {
                version: page.context.revision.to_string(),
                threads: page
                    .threads
                    .into_iter()
                    .map(|value| {
                        let location = snapshot
                            .as_ref()
                            .and_then(|snapshot| value.get_display_location(snapshot));
                        let status = snapshot
                            .as_ref()
                            .map(|snapshot| value.get_display_status(snapshot))
                            .unwrap_or_default();
                        let mut wire = value.into_versioned_proto(page.context.revision)?;
                        wire.display_location = location.map(m::Anchor::into_proto).into();
                        wire.display_status = status.into();
                        Ok::<_, ConnectError>(wire)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                comments: page
                    .comments
                    .into_iter()
                    .map(|value| m::Comment::into_versioned_proto(value, page.context.revision))
                    .collect::<Result<Vec<_>, _>>()?,
                next_page_token: next,
                ..Default::default()
            },
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/add_comment"))]
    async fn add_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, AddCommentRequest>,
    ) -> ServiceResult<impl Encodable<AddCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_fields(
            &request.expected_version,
            &request.request_key,
            matches!(&scope, m::ReviewScope::Review { .. }),
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(ConnectError::from)?;
        let supplied_anchor = request.anchor.as_option().cloned().unwrap_or_default();
        let anchor = m::Anchor::from_proto(&supplied_anchor)?;
        let evidence = if matches!(anchor, m::Anchor::General) {
            None
        } else {
            if supplied_anchor.snapshot_id.is_empty() {
                return Err(ConnectError::invalid_argument(
                    "File and line comments require anchor.snapshotId. Update/restart your client and reopen the review.",
                ));
            }
            let revision = uuid::Uuid::parse_str(&supplied_anchor.snapshot_id).map_err(|_| {
                ConnectError::invalid_argument(
                    "anchor.snapshotId must be a UUID for a saved review revision",
                )
            })?;
            Some(
                self.reviews
                    .get_snapshot_location(&selected.id, &revision, &anchor)
                    .await
                    .map_err(ConnectError::from)?,
            )
        };
        let v = self
            .reviews
            .add_comment(
                &selected.id,
                &who,
                &request.body,
                anchor,
                evidence,
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?;
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(ConnectError::from)?;
        let sync_error = if v.forge_comment_id.is_some()
            && v.posting.state() == crate::database::types::PostingState::Confirmed
        {
            String::new()
        } else {
            self.forge
                .sync_draft(&selected.id, &v.id, &who)
                .await
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        };
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(ConnectError::from)?;
        let wire = {
            let accepted = self
                .reviews
                .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
                .await
                .map_err(ConnectError::from)?;
            m::Comment::into_versioned_proto(v, accepted)?
        };
        Response::ok(AddCommentResponse {
            sync_error,
            comment: Some(wire).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/sync_draft_comment"))]
    async fn sync_draft_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SyncDraftCommentRequest>,
    ) -> ServiceResult<impl Encodable<SyncDraftCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let v = self
            .forge
            .sync_draft(&id(&request.review_id)?, &id(&request.comment_id)?, &who)
            .await
            .map_err(ConnectError::from)?;
        let version = self
            .reviews
            .get_context_by_id(&v.context_id)
            .await
            .map_err(ConnectError::from)?
            .revision;
        Response::ok(SyncDraftCommentResponse {
            comment: Some(m::Comment::into_versioned_proto(v, version)?).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/reply"))]
    async fn reply<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ReplyRequest>,
    ) -> ServiceResult<impl Encodable<ReplyResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_object_version(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Thread,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(ConnectError::from)?;
        let v = self
            .reviews
            .reply(
                &selected.id,
                &id(&request.thread_id)?,
                &who,
                &request.body,
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?;
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(ConnectError::from)?;
        let sync_error = if v.forge_comment_id.is_some()
            && v.posting.state() == crate::database::types::PostingState::Confirmed
        {
            String::new()
        } else {
            self.forge
                .sync_draft(&selected.id, &v.id, &who)
                .await
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        };
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(ConnectError::from)?;
        let wire = {
            let accepted = self
                .reviews
                .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
                .await
                .map_err(ConnectError::from)?;
            m::Comment::into_versioned_proto(v, accepted)?
        };
        Response::ok(ReplyResponse {
            sync_error,
            comment: Some(wire).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/edit_comment"))]
    async fn edit_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, EditCommentRequest>,
    ) -> ServiceResult<impl Encodable<EditCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_object_version(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Comment,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(ConnectError::from)?;
        let saved = match self
            .forge
            .apply_change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Edit {
                    comment_id: id(&request.comment_id)?,
                    body: request.body.clone(),
                },
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?
        {
            m::ForgeChangeObject::Comment(value) => *value,
            _ => return Err(ConnectError::internal("Unexpected edit result")),
        };
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(EditCommentResponse {
            comment: Some(m::Comment::into_versioned_proto(saved, accepted)?).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/delete_comment"))]
    async fn delete_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, DeleteCommentRequest>,
    ) -> ServiceResult<impl Encodable<DeleteCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_object_version(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Comment,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(ConnectError::from)?;
        self.forge
            .apply_change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Delete {
                    comment_id: id(&request.comment_id)?,
                },
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?;
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(DeleteCommentResponse {
            accepted_version: accepted.to_string(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/resolve_thread"))]
    async fn resolve_thread<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ResolveThreadRequest>,
    ) -> ServiceResult<impl Encodable<ResolveThreadResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let guard = m::MutationGuard::from_proto_object_version(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Thread,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(ConnectError::from)?;
        let saved = match self
            .forge
            .apply_change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Resolve {
                    thread_id: id(&request.thread_id)?,
                    resolved: request.resolved,
                },
                Some(&guard),
            )
            .await
            .map_err(ConnectError::from)?
        {
            m::ForgeChangeObject::Thread(value) => *value,
            _ => return Err(ConnectError::internal("Unexpected resolution result")),
        };
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(ResolveThreadResponse {
            thread: Some(m::Thread::into_versioned_proto(saved, accepted)?).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/export"))]
    async fn export<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ExportRequest>,
    ) -> ServiceResult<impl Encodable<ExportResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let scope = m::ReviewScope::from_proto(&request.scope)?;
        let ids = request
            .comment_ids
            .iter()
            .map(|value| id(value))
            .collect::<Result<Vec<_>, _>>()?;
        Response::ok(ExportResponse {
            markdown: self
                .reviews
                .get_export(&scope, &ids)
                .await
                .map_err(ConnectError::from)?,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/list_pull_requests"))]
    async fn list_pull_requests<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ListPullRequestsRequest>,
    ) -> ServiceResult<impl Encodable<ListPullRequestsResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let forge = &self.forge;
        let (rows, next_page) = forge
            .get_pull_requests(&request.repository_url, &request.query, request.page)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(ListPullRequestsResponse {
            pull_requests: rows
                .into_iter()
                .map(|v| PullRequest {
                    number: v.number,
                    title: v.title,
                    head: v.head,
                    base: v.base,
                    repository_url: v.repository_url,
                    ..Default::default()
                })
                .collect(),
            next_page,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/submit_review"))]
    async fn submit_review<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SubmitReviewRequest>,
    ) -> ServiceResult<impl Encodable<SubmitReviewResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let guard = m::MutationGuard::from_proto_fields(
            &request.expected_version,
            &request.request_key,
            true,
        )?;
        if request.expected_base.is_empty() || request.expected_head.is_empty() {
            return Err(ConnectError::invalid_argument(
                "Submission requires both reviewed commits",
            ));
        }
        let v = self
            .forge
            .submit_review(
                &id(&request.review_id)?,
                &who,
                m::ReviewSubmission {
                    outcome: request.outcome,
                    body: request.body,
                    request_key: request.request_key,
                    expected_head: request.expected_head,
                    expected_base: Some(request.expected_base),
                    expected_version: guard.expected_version,
                },
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(SubmitReviewResponse {
            review: Some(m::Review::into_proto(
                self.reviews
                    .get_review(&v.id)
                    .await
                    .map_err(ConnectError::from)?,
            ))
            .into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/reconcile_submission"))]
    async fn reconcile_submission<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ReconcileSubmissionRequest>,
    ) -> ServiceResult<impl Encodable<ReconcileSubmissionResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let who = self
            .reviews
            .auth()
            .requester(&ctx)
            .await
            .map_err(ConnectError::from)?;
        let v = self
            .forge
            .reconcile(&id(&request.review_id)?, &who)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(ReconcileSubmissionResponse {
            review: Some(m::Review::into_proto(
                self.reviews
                    .get_review(&v.id)
                    .await
                    .map_err(ConnectError::from)?,
            ))
            .into(),
            ..Default::default()
        })
    }
}
