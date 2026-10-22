//! Typed Connect/gRPC boundary over the existing application services.
mod diff;
use crate::{
    database::Database,
    models as m,
    proto::lgtm::v1::*,
    services::{AuthService, ReviewService as Reviews},
};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};
use uuid::Uuid;

pub struct Api {
    pub reviews: Reviews,
    forge: crate::services::forge::ForgeService,
    diffs: crate::services::diff::DiffService,
}
impl Api {
    pub fn new(database: Database) -> Self {
        let diffs = crate::services::diff::DiffService::new();
        let reviews =
            Reviews::new(database.clone(), AuthService::new(database)).with_sources(diffs.clone());
        Self {
            forge: crate::services::forge::ForgeService::new(reviews.clone()),
            diffs,
            reviews,
        }
    }
}
fn error(error: crate::error::Error) -> ConnectError {
    match error {
        crate::error::Error::Invalid(message) => ConnectError::invalid_argument(message),
        crate::error::Error::NotFound(message) => ConnectError::not_found(message),
        crate::error::Error::Conflict(message) => ConnectError::failed_precondition(message),
        other => {
            tracing::error!(error = %other, "RPC failed");
            ConnectError::internal("Backend operation failed")
        }
    }
}
fn id(value: &str) -> Result<Uuid, ConnectError> {
    Uuid::parse_str(value).map_err(|_| ConnectError::invalid_argument("Expected UUID"))
}
fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}
fn actor(v: m::Actor) -> Actor {
    let mut out = Actor {
        id: v.id.to_string(),
        name: v.name,
        ..Default::default()
    };
    match v.kind {
        m::ActorKind::Human { email } => {
            out.kind = "human".into();
            out.email = email;
        }
        m::ActorKind::Agent { model, provider } => {
            out.kind = "agent".into();
            out.model = model;
            out.provider = provider;
        }
    }
    out
}
fn anchor(v: m::Anchor) -> Anchor {
    let mut out = Anchor::default();
    match v {
        m::Anchor::General => out.kind = "general".into(),
        m::Anchor::File { path } => {
            out.kind = "file".into();
            out.path = path;
        }
        m::Anchor::Line {
            path,
            start,
            end,
            side,
        } => {
            out.kind = "line".into();
            out.path = path;
            out.start = start;
            out.end = end;
            out.side = side;
        }
    }
    out
}
fn input_anchor(v: &Anchor) -> Result<m::Anchor, ConnectError> {
    if (v.kind != "line" && (v.start != 0 || v.end != 0 || !v.side.is_empty()))
        || ((v.kind.is_empty() || v.kind == "general") && !v.path.is_empty())
    {
        return Err(ConnectError::invalid_argument(
            "Anchor fields do not match its kind",
        ));
    }
    let out = match v.kind.as_str() {
        "" | "general" => m::Anchor::General,
        "file" => m::Anchor::File {
            path: v.path.clone(),
        },
        "line" => m::Anchor::Line {
            path: v.path.clone(),
            start: v.start,
            end: if v.end == 0 { v.start } else { v.end },
            side: if v.side.is_empty() {
                "new".into()
            } else {
                v.side.clone()
            },
        },
        _ => return Err(ConnectError::invalid_argument("Unknown anchor kind")),
    };
    out.validate().map_err(error)?;
    Ok(out)
}
fn input_target(v: &ReviewTarget) -> Result<m::ReviewTarget, ConnectError> {
    match &v.kind {
        Some(review_target::Kind::LocalGit(v)) => Ok(m::ReviewTarget::LocalGit {
            path: v.path.clone(),
            branch: optional(&v.branch),
        }),
        Some(review_target::Kind::Directory(v)) => Ok(m::ReviewTarget::Directory {
            path: v.path.clone(),
        }),
        Some(review_target::Kind::PullRequest(v)) => Ok(m::ReviewTarget::PullRequest {
            repository_url: v.repository_url.clone(),
            number: v.number,
        }),
        None => Err(ConnectError::invalid_argument("Select a review target")),
    }
}

fn input_comparison(v: &ReviewComparison) -> Result<Option<m::ReviewComparison>, ConnectError> {
    let out = match &v.kind {
        Some(review_comparison::Kind::Working(v)) => m::ReviewComparison::Working {
            base_commit: v.base_commit.clone(),
        },
        Some(review_comparison::Kind::Staged(v)) => m::ReviewComparison::Staged {
            base_commit: v.base_commit.clone(),
        },
        Some(review_comparison::Kind::Branch(v)) => m::ReviewComparison::Branch {
            base_commit: v.base_commit.clone(),
            head_commit: v.head_commit.clone(),
        },
        Some(review_comparison::Kind::Directory(_)) => m::ReviewComparison::Directory,
        Some(review_comparison::Kind::PullRequest(v)) => m::ReviewComparison::PullRequest {
            base_commit: v.base_commit.clone(),
            head_commit: v.head_commit.clone(),
        },
        None => return Ok(None),
    };
    out.validate().map_err(error)?;
    Ok(Some(out))
}

fn input_scope(v: &ReviewScope) -> Result<m::ReviewScope, ConnectError> {
    match &v.kind {
        Some(review_scope::Kind::ReviewId(v)) => Ok(m::ReviewScope::Review { review_id: id(v)? }),
        Some(review_scope::Kind::Notes(v)) => Ok(m::ReviewScope::Notes {
            path: v.path.clone(),
        }),
        None => Err(ConnectError::invalid_argument(
            "Select a review or notes scope",
        )),
    }
}

fn input_guard(
    version: &str,
    key: &str,
    require_version: bool,
) -> Result<m::MutationGuard, ConnectError> {
    if key.trim().is_empty() || key.len() > 200 {
        return Err(ConnectError::invalid_argument(
            "Provide a bounded nonempty request key",
        ));
    }
    let expected = if version.is_empty() {
        if require_version {
            return Err(ConnectError::invalid_argument(
                "Read the current version before writing",
            ));
        }
        None
    } else {
        Some(
            version
                .parse::<i64>()
                .ok()
                .filter(|v| *v >= 0)
                .ok_or_else(|| ConnectError::invalid_argument("Invalid saved-data version"))?,
        )
    };
    Ok(m::MutationGuard::new(expected, key.into()))
}

fn input_object_guard(
    version: &str,
    key: &str,
    kind: m::ObjectKind,
) -> Result<m::MutationGuard, ConnectError> {
    let parts: Vec<_> = version.split(':').collect();
    if parts.len() != 4
        || parts[0] != kind.as_str()
        || parts[3].len() != 64
        || !parts[3].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(ConnectError::invalid_argument(
            "Use the selected comment or thread version returned by GetConversation",
        ));
    }
    Ok(
        input_guard(parts[1], key, true)?.with_expected_object(m::ExpectedObject {
            kind,
            id: id(parts[2])?,
            hash: parts[3].into(),
        }),
    )
}

fn versioned_comment(value: m::Comment, version: i64) -> Result<Comment, ConnectError> {
    let token = crate::utils::object_version(m::ObjectKind::Comment, version, value.id, &value)
        .map_err(error)?;
    let mut value = comment(value);
    value.version = token;
    Ok(value)
}

fn versioned_thread(value: m::Thread, version: i64) -> Result<Thread, ConnectError> {
    let token = crate::utils::object_version(m::ObjectKind::Thread, version, value.id, &value)
        .map_err(error)?;
    let mut value = thread(value);
    value.version = token;
    Ok(value)
}

fn output_target(value: m::ReviewTarget) -> ReviewTarget {
    match value {
        m::ReviewTarget::LocalGit { path, branch } => ReviewTarget {
            kind: Some(
                LocalGitTarget {
                    path,
                    branch: branch.unwrap_or_default(),
                    ..Default::default()
                }
                .into(),
            ),
            ..Default::default()
        },
        m::ReviewTarget::Directory { path } => ReviewTarget {
            kind: Some(
                DirectoryTarget {
                    path,
                    ..Default::default()
                }
                .into(),
            ),
            ..Default::default()
        },
        m::ReviewTarget::PullRequest {
            repository_url,
            number,
        } => ReviewTarget {
            kind: Some(
                PullRequestTarget {
                    repository_url,
                    number,
                    ..Default::default()
                }
                .into(),
            ),
            ..Default::default()
        },
    }
}

fn output_comparison(c: m::ReviewComparison) -> ReviewComparison {
    ReviewComparison {
        kind: Some(match c {
            m::ReviewComparison::Working { base_commit } => WorkingComparison {
                base_commit,
                ..Default::default()
            }
            .into(),
            m::ReviewComparison::Staged { base_commit } => StagedComparison {
                base_commit,
                ..Default::default()
            }
            .into(),
            m::ReviewComparison::Branch {
                base_commit,
                head_commit,
            } => BranchComparison {
                base_commit,
                head_commit,
                ..Default::default()
            }
            .into(),
            m::ReviewComparison::Directory => DirectoryComparison::default().into(),
            m::ReviewComparison::PullRequest {
                base_commit,
                head_commit,
            } => PullRequestComparison {
                base_commit,
                head_commit,
                ..Default::default()
            }
            .into(),
        }),
        ..Default::default()
    }
}

fn review(v: m::Review) -> Review {
    let target = output_target(v.target);
    let comparison = v.comparison.map(output_comparison);
    let completion = v.session.completion().cloned();
    Review {
        id: v.session.id.to_string(),
        target: Some(target).into(),
        comparison: comparison.into(),
        observed_comparison: v.observed_comparison.map(output_comparison).into(),
        state: v.session.state().into(),
        version: v.version.to_string(),
        forge_state: v
            .forge_state
            .map(|state| state.as_str().to_owned())
            .unwrap_or_default(),
        creator: Some(actor(v.session.creator)).into(),
        created_at: v.session.created_at,
        outcome: completion
            .as_ref()
            .map(|c| c.outcome.clone())
            .unwrap_or_default(),
        body: completion
            .and_then(|c| c.body)
            .or(v.session.draft_body)
            .unwrap_or_default(),
        submission_status: v.session.submission_status,
        submission_error: v.session.submission_error.unwrap_or_default(),
        forge_review_id: v.session.forge_review_id.unwrap_or_default(),
        ..Default::default()
    }
}

fn input_source_change(v: &SourceChange) -> Result<m::SourceChange, ConnectError> {
    match &v.kind {
        Some(source_change::Kind::LocationMoved(v)) => Ok(m::SourceChange::LocationMoved {
            previous_path: v.previous_path.clone(),
            current_path: v.current_path.clone(),
        }),
        Some(source_change::Kind::Moved(v)) => Ok(m::SourceChange::Moved {
            thread_id: id(&v.thread_id)?,
            previous_path: v.previous_path.clone(),
            current_path: v.current_path.clone(),
            source: input_source(&v.source)?,
        }),
        Some(source_change::Kind::Deleted(v)) => Ok(m::SourceChange::Deleted {
            thread_id: id(&v.thread_id)?,
            path: v.path.clone(),
        }),
        Some(source_change::Kind::WorkingSourceChanged(v)) => {
            Ok(m::SourceChange::WorkingSourceChanged {
                thread_id: id(&v.thread_id)?,
                path: v.path.clone(),
                source: input_source(&v.source)?,
            })
        }
        Some(source_change::Kind::Restored(v)) => Ok(m::SourceChange::Restored {
            thread_id: id(&v.thread_id)?,
            path: v.path.clone(),
            source: input_source(&v.source)?,
        }),
        None => Err(ConnectError::invalid_argument(
            "Source change requires a typed observation",
        )),
    }
}

fn input_source(v: &AnchorSource) -> Result<m::AnchorSource, ConnectError> {
    let source = match &v.kind {
        Some(anchor_source::Kind::Commit(v)) => m::AnchorSource::Commit {
            sha: v.sha.clone(),
            content_hash: v.content_hash.clone(),
        },
        Some(anchor_source::Kind::IndexFile(v)) => m::AnchorSource::IndexFile {
            blob_oid: v.blob_oid.clone(),
            content_hash: v.content_hash.clone(),
        },
        Some(anchor_source::Kind::WorkingFile(v)) => m::AnchorSource::WorkingFile {
            content_hash: v.content_hash.clone(),
        },
        Some(anchor_source::Kind::LocalFile(v)) => m::AnchorSource::LocalFile {
            content_hash: v.content_hash.clone(),
        },
        None => {
            return Err(ConnectError::invalid_argument(
                "Anchor source requires a typed identity",
            ));
        }
    };
    source.validate().map_err(error)?;
    Ok(source)
}

fn source(v: m::AnchorSource) -> AnchorSource {
    AnchorSource {
        kind: Some(match v {
            m::AnchorSource::Commit { sha, content_hash } => CommitSource {
                sha,
                content_hash,
                ..Default::default()
            }
            .into(),
            m::AnchorSource::IndexFile {
                blob_oid,
                content_hash,
            } => IndexFileSource {
                blob_oid,
                content_hash,
                ..Default::default()
            }
            .into(),
            m::AnchorSource::WorkingFile { content_hash } => WorkingFileSource {
                content_hash,
                ..Default::default()
            }
            .into(),
            m::AnchorSource::LocalFile { content_hash } => LocalFileSource {
                content_hash,
                ..Default::default()
            }
            .into(),
        }),
        ..Default::default()
    }
}

fn input_evidence(v: &Anchor) -> Result<Option<m::AnchorEvidence>, ConnectError> {
    if v.source.is_set() != v.evidence.is_set() {
        return Err(ConnectError::invalid_argument(
            "Anchor evidence requires its source and excerpt fields",
        ));
    }
    if !v.source.is_set() {
        return Ok(None);
    }
    if !["line", "file"].contains(&v.kind.as_str()) {
        return Err(ConnectError::invalid_argument(
            "Source evidence requires a file or line anchor",
        ));
    }
    let evidence = m::AnchorEvidence {
        source: input_source(&v.source)?,
        selected_text: v.evidence.selected_text.clone(),
        context_before: v.evidence.context_before.clone(),
        context_after: v.evidence.context_after.clone(),
    };
    if v.kind == "file"
        && (!evidence.selected_text.is_empty()
            || !evidence.context_before.is_empty()
            || !evidence.context_after.is_empty())
    {
        return Err(ConnectError::invalid_argument(
            "File evidence has no line selection",
        ));
    }
    evidence.validate().map_err(error)?;
    Ok(Some(evidence))
}

fn comment(v: m::Comment) -> Comment {
    Comment {
        id: v.id.to_string(),
        context_id: v.context_id.to_string(),
        thread_id: v.thread_id.to_string(),
        origin_session_id: v
            .origin_session_id
            .map(|x| x.to_string())
            .unwrap_or_default(),
        posting_status: v.posting.state().as_str().into(),
        owner_id: v.owner_id.map(|id| id.to_string()).unwrap_or_default(),
        author: Some(actor(v.author)).into(),
        body: v.body,
        deleted: v.deleted,
        position: v.position,
        visibility: v.visibility,
        created_at: v.created_at,
        updated_at: v.updated_at,
        forge_comment_id: v.forge_comment_id.unwrap_or_default(),
        remote_presence: v.remote_presence,
        ..Default::default()
    }
}
fn thread(v: m::Thread) -> Thread {
    let mut original = anchor(v.anchor);
    if let Some(evidence) = v.evidence {
        original.source = Some(source(evidence.source)).into();
        original.evidence = Some(AnchorEvidence {
            selected_text: evidence.selected_text,
            context_before: evidence.context_before,
            context_after: evidence.context_after,
            ..Default::default()
        })
        .into();
    }
    Thread {
        id: v.id.to_string(),
        anchor: Some(original).into(),
        current_path: v.current_path.unwrap_or_default(),
        source_path: v.source_path.unwrap_or_default(),
        orphan_reason: v
            .orphan_reason
            .map(|r| {
                match r {
                    m::OrphanReason::FileDeleted => "file_deleted",
                    m::OrphanReason::WorkingSourceChanged => "working_source_changed",
                }
                .to_owned()
            })
            .unwrap_or_default(),
        forge_observation: v
            .forge_observation
            .map(|o| ForgeObservation {
                base: o.base,
                head: o.head,
                outdated: o.outdated,
                current: o.current.map(anchor).into(),
                ..Default::default()
            })
            .into(),
        state: v.state,
        author: Some(actor(v.author)).into(),
        forge_thread_id: v.forge_thread_id.unwrap_or_default(),
        ..Default::default()
    }
}
impl IdentityService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/ensure_identity"))]
    async fn ensure_identity<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, EnsureIdentityRequest>,
    ) -> ServiceResult<impl Encodable<EnsureIdentityResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        if !request.email.is_empty() && (!request.model.is_empty() || !request.provider.is_empty())
        {
            return Err(ConnectError::invalid_argument(
                "Human email and agent model/provider are mutually exclusive",
            ));
        }
        let v = if request.model.is_empty() && request.provider.is_empty() {
            self.reviews
                .auth()
                .ensure_human(&request.name, &request.email)
                .await
        } else {
            self.reviews
                .auth()
                .ensure_agent(&request.name, &request.model, &request.provider)
                .await
        }
        .map_err(error)?;
        Response::ok(EnsureIdentityResponse {
            actor: Some(actor(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_capabilities"))]
    async fn get_capabilities<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetCapabilitiesRequest>,
    ) -> ServiceResult<impl Encodable<GetCapabilitiesResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let _ = request;
        Response::ok(GetCapabilitiesResponse {
            api_version: "1".into(),
            capabilities: vec![
                "notes".into(),
                "local_review".into(),
                "export".into(),
                "github_pending_drafts".into(),
                "review_targets".into(),
                "feedback_scopes".into(),
                "anchor_evidence".into(),
                "diff_service".into(),
                "rejected_submission_recovery".into(),
            ],
            ..Default::default()
        })
    }
}
impl ReviewService for Api {
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let mut guard = input_guard(
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
        let target = input_target(&request.target)?;
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
        let supplied = input_comparison(&request.comparison)?;
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
        let mut context = self.reviews.resolve_target(&target).await.map_err(error)?;
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
                .map_err(error)?
            {
                return Response::ok(StartReviewResponse {
                    review: Some(review(
                        self.reviews.get_review(&saved.id).await.map_err(error)?,
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
                .map_err(error)?;
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
                    .map_err(error)?
            } else if let Some(version) = guard.expected_version {
                self.forge
                    .refresh_at_version(&context.id, version)
                    .await
                    .map_err(error)?
            } else {
                self.forge.refresh(&context.id).await.map_err(error)?
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
            .map_err(error)?;
        Response::ok(StartReviewResponse {
            review: Some(review(
                self.reviews.get_review(&saved.id).await.map_err(error)?,
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
            review: Some(review(
                self.reviews
                    .get_review(&id(request.review_id)?)
                    .await
                    .map_err(error)?,
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
        let target = request.target.as_option().map(input_target).transpose()?;
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
            .map_err(error)?;
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
            reviews: values.into_iter().map(review).collect(),
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_guard(&request.expected_version, &request.request_key, true)?;
        let changes = request
            .source_changes
            .iter()
            .map(input_source_change)
            .collect::<Result<Vec<_>, _>>()?;
        let version = self
            .reviews
            .refresh_locations(&scope, &who, &changes, &guard)
            .await
            .map_err(error)?;
        let refreshed = if let m::ReviewScope::Review { review_id } = scope {
            let selected = self.reviews.get_session(&review_id).await.map_err(error)?;
            if self
                .reviews
                .get_context_by_id(&selected.context_id)
                .await
                .map_err(error)?
                .key
                .is_forge_backed()
            {
                self.forge
                    .refresh(&selected.context_id)
                    .await
                    .map_err(error)?;
            }
            Some(review(
                self.reviews.get_review(&review_id).await.map_err(error)?,
            ))
        } else {
            None
        };
        let version = refreshed
            .as_ref()
            .map(|r| r.version.clone())
            .unwrap_or_else(|| version.to_string());
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let guard = input_guard(&request.expected_version, &request.request_key, true)?;
        let selected = self
            .reviews
            .get_session(&id(&request.review_id)?)
            .await
            .map_err(error)?;
        if self
            .reviews
            .get_context_by_id(&selected.context_id)
            .await
            .map_err(error)?
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
            .map_err(error)?;
        Response::ok(CompleteReviewResponse {
            review: Some(review(
                self.reviews.get_review(&saved.id).await.map_err(error)?,
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
        let scope = input_scope(&request.scope)?;
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
            .map_err(error)?;
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
                    .map(|value| versioned_thread(value, page.context.revision))
                    .collect::<Result<Vec<_>, _>>()?,
                comments: page
                    .comments
                    .into_iter()
                    .map(|value| versioned_comment(value, page.context.revision))
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_guard(
            &request.expected_version,
            &request.request_key,
            matches!(&scope, m::ReviewScope::Review { .. }),
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(error)?;
        let mut supplied_anchor = request.anchor.as_option().cloned().unwrap_or_default();
        if let m::ReviewScope::Notes { path } = &scope {
            let context = self
                .reviews
                .get_context_by_id(&selected.context_id)
                .await
                .map_err(error)?;
            let m::ContextKey::Local {
                location: m::LocalLocation::Notes { path: original },
            } = &context.key
            else {
                return Err(ConnectError::invalid_argument("Expected notes context"));
            };
            let original_path =
                std::path::Path::new(context.current_notes_path.as_ref().unwrap_or(original));
            let root = if original_path.is_dir() {
                original_path
            } else {
                original_path
                    .parent()
                    .ok_or_else(|| ConnectError::invalid_argument("Notes file has no parent"))?
            };
            let selected_path = std::path::Path::new(path)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(path));
            let relative = selected_path
                .strip_prefix(root)
                .or_else(|_| {
                    selected_path
                        .file_name()
                        .map(std::path::Path::new)
                        .ok_or(())
                })
                .map_err(|_| ConnectError::invalid_argument("Notes file has no name"))?
                .to_string_lossy()
                .into_owned();
            if ["file", "line"].contains(&supplied_anchor.kind.as_str()) {
                if !supplied_anchor.path.is_empty() && supplied_anchor.path != relative {
                    return Err(ConnectError::invalid_argument(
                        "Notes anchor differs from its file scope",
                    ));
                }
                supplied_anchor.path = relative;
            }
        }
        let evidence = input_evidence(&supplied_anchor)?;
        if supplied_anchor.kind == "line" && evidence.is_none() {
            return Err(ConnectError::invalid_argument(
                "Capture the actual displayed source before adding a line comment",
            ));
        }
        let v = self
            .reviews
            .add_comment(
                &selected.id,
                &who,
                &request.body,
                input_anchor(&supplied_anchor)?,
                evidence,
                Some(&guard),
            )
            .await
            .map_err(error)?;
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(error)?;
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
            .map_err(error)?;
        let wire = {
            let accepted = self
                .reviews
                .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
                .await
                .map_err(error)?;
            versioned_comment(v, accepted)?
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let v = self
            .forge
            .sync_draft(&id(&request.review_id)?, &id(&request.comment_id)?, &who)
            .await
            .map_err(error)?;
        let version = self
            .reviews
            .get_context_by_id(&v.context_id)
            .await
            .map_err(error)?
            .revision;
        Response::ok(SyncDraftCommentResponse {
            comment: Some(versioned_comment(v, version)?).into(),
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_object_guard(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Thread,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(error)?;
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
            .map_err(error)?;
        let v = self
            .reviews
            .get_comment(&v.context_id, &v.id)
            .await
            .map_err(error)?;
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
            .map_err(error)?;
        let wire = {
            let accepted = self
                .reviews
                .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
                .await
                .map_err(error)?;
            versioned_comment(v, accepted)?
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_object_guard(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Comment,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(error)?;
        let saved = match self
            .forge
            .change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Edit {
                    comment_id: id(&request.comment_id)?,
                    body: request.body.clone(),
                },
                Some(&guard),
            )
            .await
            .map_err(error)?
        {
            m::ForgeChangeObject::Comment(value) => *value,
            _ => return Err(ConnectError::internal("Unexpected edit result")),
        };
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(error)?;
        Response::ok(EditCommentResponse {
            comment: Some(versioned_comment(saved, accepted)?).into(),
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_object_guard(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Comment,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(error)?;
        self.forge
            .change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Delete {
                    comment_id: id(&request.comment_id)?,
                },
                Some(&guard),
            )
            .await
            .map_err(error)?;
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(error)?;
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let scope = input_scope(&request.scope)?;
        let guard = input_object_guard(
            &request.expected_version,
            &request.request_key,
            m::ObjectKind::Thread,
        )?
        .in_scope(scope.clone());
        let selected = self
            .reviews
            .resolve_write_scope(&scope, &who)
            .await
            .map_err(error)?;
        let saved = match self
            .forge
            .change(
                &selected.id,
                &who,
                m::ForgeChangeInput::Resolve {
                    thread_id: id(&request.thread_id)?,
                    resolved: request.resolved,
                },
                Some(&guard),
            )
            .await
            .map_err(error)?
        {
            m::ForgeChangeObject::Thread(value) => *value,
            _ => return Err(ConnectError::internal("Unexpected resolution result")),
        };
        let accepted = self
            .reviews
            .get_accepted_version(&selected.context_id, &who.id, &request.request_key)
            .await
            .map_err(error)?;
        Response::ok(ResolveThreadResponse {
            thread: Some(versioned_thread(saved, accepted)?).into(),
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
        let scope = input_scope(&request.scope)?;
        let ids = request
            .comment_ids
            .iter()
            .map(|value| id(value))
            .collect::<Result<Vec<_>, _>>()?;
        Response::ok(ExportResponse {
            markdown: self.reviews.get_export(&scope, &ids).await.map_err(error)?,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/promote_notes"))]
    async fn promote_notes<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, PromoteNotesRequest>,
    ) -> ServiceResult<impl Encodable<PromoteNotesResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let ids = request
            .comment_ids
            .iter()
            .map(|v| id(v))
            .collect::<Result<Vec<_>, _>>()?;
        let anchors = request
            .anchors
            .iter()
            .map(input_anchor)
            .collect::<Result<Vec<_>, _>>()?;
        let source = input_scope(&request.source)?;
        if !matches!(source, m::ReviewScope::Notes { .. }) {
            return Err(ConnectError::invalid_argument(
                "Promotion source must be a notes location",
            ));
        }
        let context = self
            .reviews
            .get_scope_context(&source)
            .await
            .map_err(error)?
            .ok_or_else(|| ConnectError::not_found("Source notes"))?;
        let guard = input_guard(&request.expected_review_version, &request.request_key, true)?;
        let source_version =
            input_guard(&request.expected_source_version, &request.request_key, true)?
                .expected_version;
        let evidence = request
            .anchors
            .iter()
            .map(input_evidence)
            .collect::<Result<Vec<_>, _>>()?;
        if anchors.iter().zip(&evidence).any(|(anchor, evidence)| {
            matches!(anchor, m::Anchor::Line { .. }) && evidence.is_none()
        }) {
            return Err(ConnectError::invalid_argument(
                "Promoted line feedback requires confirmed target source evidence",
            ));
        }
        let mut rows = self
            .reviews
            .promote_notes(
                &context.id,
                &id(&request.target_review_id)?,
                &who,
                m::NotePromotion {
                    comment_ids: ids,
                    anchors,
                    evidence,
                    request_key: request.request_key.clone(),
                    expected_source_version: source_version,
                    guard: Some(guard),
                },
            )
            .await
            .map_err(error)?;
        let target = self
            .reviews
            .get_session(&id(&request.target_review_id)?)
            .await
            .map_err(error)?;
        let accepted = self
            .reviews
            .get_accepted_version(&target.context_id, &who.id, &request.request_key)
            .await
            .map_err(error)?;
        let mut sync_errors = Vec::new();
        if self
            .reviews
            .get_context_by_id(&target.context_id)
            .await
            .map_err(error)?
            .key
            .is_forge_backed()
        {
            for row in &mut rows {
                if let Ok(saved) = self.reviews.get_comment(&target.context_id, &row.id).await {
                    *row = saved;
                }
                if row.forge_comment_id.is_some()
                    && row.posting.state() == crate::database::types::PostingState::Confirmed
                {
                    continue;
                }
                match self.forge.sync_draft(&target.id, &row.id, &who).await {
                    Ok(saved) => *row = saved,
                    Err(error) => {
                        sync_errors.push(CommentSyncFailure {
                            comment_id: row.id.to_string(),
                            message: error.to_string(),
                            ..Default::default()
                        });
                        if let Ok(saved) =
                            self.reviews.get_comment(&target.context_id, &row.id).await
                        {
                            *row = saved;
                        }
                    }
                }
            }
        }
        Response::ok(PromoteNotesResponse {
            comments: rows
                .into_iter()
                .map(|value| versioned_comment(value, accepted))
                .collect::<Result<Vec<_>, _>>()?,
            accepted_version: accepted.to_string(),
            sync_errors,
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
            .map_err(error)?;
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let guard = input_guard(&request.expected_version, &request.request_key, true)?;
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
            .map_err(error)?;
        Response::ok(SubmitReviewResponse {
            review: Some(review(self.reviews.get_review(&v.id).await.map_err(error)?)).into(),
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
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let v = self
            .forge
            .reconcile(&id(&request.review_id)?, &who)
            .await
            .map_err(error)?;
        Response::ok(ReconcileSubmissionResponse {
            review: Some(review(self.reviews.get_review(&v.id).await.map_err(error)?)).into(),
            ..Default::default()
        })
    }
}
